//! Lifecycle coordination shared by request-owned and daemon-owned commands.
//! Ownership is released only after cancelled commands have been reaped.
use crate::{config::DaemonConfig, logs::append_log_logged};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    io,
    process::{Output, Stdio},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{process::Command, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug)]
pub enum LifecycleError {
    Busy,
    Cancelled,
    Failed(String),
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("A Project lifecycle operation is already in progress"),
            Self::Cancelled => f.write_str("Project lifecycle operation cancelled"),
            Self::Failed(error) => f.write_str(error),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Accepted,
    WaitingForCleanup,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationView {
    pub id: Uuid,
    pub project_id: Uuid,
    pub request_id: Option<Uuid>,
    pub action: String,
    pub state: OperationState,
    pub accepted_at: u64,
    pub finished_at: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationSnapshot {
    pub daemon_instance_id: Uuid,
    pub active: Option<OperationView>,
    pub recent: Vec<OperationView>,
}

struct OperationRegistry {
    instance: Uuid,
    active: HashMap<Uuid, Arc<Operation>>,
    recent: HashMap<Uuid, VecDeque<OperationView>>,
}

#[derive(Clone)]
pub struct Operations(Arc<Mutex<OperationRegistry>>);

impl Default for Operations {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(OperationRegistry {
            instance: Uuid::new_v4(),
            active: HashMap::new(),
            recent: HashMap::new(),
        })))
    }
}

struct Operation {
    stopping: bool,
    cancel: CancellationToken,
    done: CancellationToken,
    view: Mutex<OperationView>,
}

struct StartedOperation {
    operation: Arc<Operation>,
    task: JoinHandle<Result<(), LifecycleError>>,
    receipt: OperationView,
}

struct OperationGuard {
    operations: Operations,
    project: Uuid,
    operation: Arc<Operation>,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let mut registry = self.operations.0.lock().unwrap();
        if registry
            .active
            .get(&self.project)
            .is_some_and(|entry| Arc::ptr_eq(entry, &self.operation))
        {
            registry.active.remove(&self.project);
        }
        self.operation.done.cancel();
    }
}

impl Operations {
    pub fn snapshot(&self, project: Uuid) -> OperationSnapshot {
        let registry = self.0.lock().unwrap();
        OperationSnapshot {
            daemon_instance_id: registry.instance,
            active: registry
                .active
                .get(&project)
                .map(|operation| operation.view.lock().unwrap().clone()),
            recent: registry
                .recent
                .get(&project)
                .map(|records| records.iter().cloned().collect())
                .unwrap_or_default(),
        }
    }

    /// Request-owned calls preserve cancellation when the awaiting handler drops.
    pub async fn run<F, Fut>(
        &self,
        config: &DaemonConfig,
        project: Uuid,
        stopping: bool,
        label: &'static str,
        work: F,
    ) -> Result<(), LifecycleError>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let started = self.start(config, project, stopping, label, None, false, work)?;
        let request_guard = started.operation.cancel.clone().drop_guard();
        let result = started
            .task
            .await
            .map_err(|error| LifecycleError::Failed(format!("Lifecycle task failed: {error}")))?;
        request_guard.disarm();
        result
    }

    /// Reserve and start the same coordinated work without binding it to a request.
    pub fn submit<F, Fut>(
        &self,
        config: &DaemonConfig,
        project: Uuid,
        stopping: bool,
        label: &'static str,
        request_id: Uuid,
        work: F,
    ) -> Result<OperationView, LifecycleError>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let started = self.start(
            config,
            project,
            stopping,
            label,
            Some(request_id),
            true,
            work,
        )?;
        Ok(started.receipt)
    }

    /// A stop reserves its successor before cancelling active work; every caller
    /// shares this map and waits for the predecessor's complete process cleanup.
    fn start<F, Fut>(
        &self,
        config: &DaemonConfig,
        project: Uuid,
        stopping: bool,
        label: &'static str,
        request_id: Option<Uuid>,
        detached: bool,
        work: F,
    ) -> Result<StartedOperation, LifecycleError>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let (operation, previous, receipt) = {
            let mut registry = self.0.lock().unwrap();
            let previous = registry.active.get(&project).cloned();
            if let Some(active) = &previous {
                if !stopping || active.stopping {
                    return Err(LifecycleError::Busy);
                }
            }
            if let Some(request_id) = request_id {
                let is_duplicate = previous.as_ref().is_some_and(|active| {
                    active.view.lock().unwrap().request_id == Some(request_id)
                }) || registry.recent.get(&project).is_some_and(|records| {
                    records
                        .iter()
                        .any(|record| record.request_id == Some(request_id))
                });
                if is_duplicate {
                    return Err(LifecycleError::Busy);
                }
            }
            let receipt = OperationView {
                id: Uuid::new_v4(),
                project_id: project,
                request_id,
                action: label.to_owned(),
                state: if previous.is_some() {
                    OperationState::WaitingForCleanup
                } else {
                    OperationState::Accepted
                },
                accepted_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                finished_at: None,
                error: None,
            };
            let operation = Arc::new(Operation {
                stopping,
                cancel: CancellationToken::new(),
                done: CancellationToken::new(),
                view: Mutex::new(receipt.clone()),
            });
            registry.active.insert(project, operation.clone());
            (operation, previous, receipt)
        };
        let guard = OperationGuard {
            operations: self.clone(),
            project,
            operation: operation.clone(),
        };
        let running = operation.clone();
        let log_dir = config.log_dir.clone();
        let registry = self.clone();
        let task = tokio::spawn(async move {
            let _guard = guard;
            if let Some(previous) = previous {
                previous.cancel.cancel();
                previous.done.cancelled().await;
            }
            let result = if running.cancel.is_cancelled() {
                Err(LifecycleError::Cancelled)
            } else {
                running.view.lock().unwrap().state = OperationState::Running;
                work(running.cancel.clone())
                    .await
                    .map_err(LifecycleError::Failed)
            };
            let result = if running.cancel.is_cancelled() {
                append_log_logged(
                    &log_dir,
                    project,
                    "daemon",
                    &format!("{label} cancelled; command cleanup finished"),
                );
                Err(LifecycleError::Cancelled)
            } else {
                result
            };
            let finished = {
                let mut view = running.view.lock().unwrap();
                view.state = match &result {
                    Ok(()) => OperationState::Succeeded,
                    Err(LifecycleError::Cancelled) => OperationState::Cancelled,
                    Err(_) => OperationState::Failed,
                };
                view.finished_at = Some(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                );
                view.error = result.as_ref().err().map(ToString::to_string);
                view.clone()
            };
            if detached {
                if let Err(LifecycleError::Failed(error)) = &result {
                    tracing::error!(project = %project, operation = %finished.id, "{error}");
                    append_log_logged(&log_dir, project, "daemon:error", error);
                }
            }
            let mut state = registry.0.lock().unwrap();
            let recent = state.recent.entry(project).or_default();
            recent.push_front(finished);
            recent.truncate(16);
            result
        });
        Ok(StartedOperation {
            operation,
            task,
            receipt,
        })
    }
}

/// Capture a real command in its own process group. Cancellation sends SIGTERM to
/// the group and waits for exit/output EOF. There is no execution or cleanup deadline.
/// Successfully detached services are not signalled after normal command completion.
pub async fn command_output(
    mut command: Command,
    cancel: &CancellationToken,
) -> io::Result<Output> {
    let cancel = cancel.child_token();
    let request_guard = cancel.clone().drop_guard();
    let task = tokio::spawn(async move {
        if cancel.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "command cancelled",
            ));
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        let child = command.spawn()?;
        let mut group = ProcessGroup(child.id().expect("spawned child has a PID") as i32);
        let output = child.wait_with_output();
        tokio::pin!(output);
        let result = tokio::select! {
            biased;
            result = &mut output => result,
            _ = cancel.cancelled() => {
                group.signal(libc::SIGTERM)?;
                // Keep the group and child owned until the command and its captured
                // descendants finish. In particular, a stop cannot race this cleanup.
                let result = output.await;
                group.0 = 0;
                result?;
                return Err(io::Error::new(io::ErrorKind::Interrupted, "command cancelled"));
            }
        };
        group.0 = 0;
        result
    });
    let result = task.await.map_err(io::Error::other)?;
    request_guard.disarm();
    result
}

struct ProcessGroup(i32);

impl ProcessGroup {
    fn signal(&self, signal: i32) -> io::Result<()> {
        // SAFETY: the positive PID came from our child, started with process_group(0).
        // Negating it addresses that command's group, never the daemon's group.
        let result = unsafe { libc::kill(-self.0, signal) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        }
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if self.0 > 0 {
            let _ = self.signal(libc::SIGKILL);
        }
    }
}
