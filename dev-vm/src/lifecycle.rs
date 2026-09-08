//! Request-owned lifecycle operations. Cancellation is signalled, not implemented by
//! dropping command futures: ownership is released only after command cleanup finishes.
use crate::{config::DaemonConfig, logs::append_log_logged};
use std::{
    collections::HashMap,
    future::Future,
    io,
    process::{Output, Stdio},
    sync::{Arc, Mutex},
};
use tokio::process::Command;
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

#[derive(Default, Clone)]
pub struct Operations(Arc<Mutex<HashMap<Uuid, Arc<Operation>>>>);

struct Operation {
    stopping: bool,
    cancel: CancellationToken,
    done: CancellationToken,
}

struct OperationGuard {
    operations: Operations,
    project: Uuid,
    operation: Arc<Operation>,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let mut operations = self.operations.0.lock().unwrap();
        if operations
            .get(&self.project)
            .is_some_and(|entry| Arc::ptr_eq(entry, &self.operation))
        {
            operations.remove(&self.project);
        }
        self.operation.done.cancel();
    }
}

impl Operations {
    /// A stop reserves the next slot before cancelling an active start, so no third
    /// request can launch between cancellation and stop. Other conflicts are rejected.
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
        let operation = Arc::new(Operation {
            stopping,
            cancel: CancellationToken::new(),
            done: CancellationToken::new(),
        });
        let previous = {
            let mut operations = self.0.lock().unwrap();
            if let Some(previous) = operations.get(&project) {
                if !stopping || previous.stopping {
                    return Err(LifecycleError::Busy);
                }
            }
            operations.insert(project, operation.clone())
        };
        let guard = OperationGuard {
            operations: self.clone(),
            project,
            operation: operation.clone(),
        };
        let request_guard = operation.cancel.clone().drop_guard();
        let log_dir = config.log_dir.clone();
        // This task is never aborted on HTTP disconnect. It owns cleanup until the
        // command is reaped, while the request's drop guard signals cancellation.
        let task = tokio::spawn(async move {
            let _guard = guard;
            if let Some(previous) = previous {
                previous.cancel.cancel();
                previous.done.cancelled().await;
            }
            let result = if operation.cancel.is_cancelled() {
                Err(LifecycleError::Cancelled)
            } else {
                work(operation.cancel.clone())
                    .await
                    .map_err(LifecycleError::Failed)
            };
            if operation.cancel.is_cancelled() {
                append_log_logged(
                    &log_dir,
                    project,
                    "daemon",
                    &format!("{label} cancelled; command cleanup finished"),
                );
                Err(LifecycleError::Cancelled)
            } else {
                result
            }
        });
        let result = task
            .await
            .map_err(|error| LifecycleError::Failed(format!("Lifecycle task failed: {error}")))?;
        request_guard.disarm();
        result
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
