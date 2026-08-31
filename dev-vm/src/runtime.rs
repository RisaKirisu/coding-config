use crate::config::DaemonConfig;
use crate::logs::append_log;
use crate::models::DshStatus;
use crate::runner::{check_vm_status, run_vm_start};
use crate::sync::SyncManager;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

const DSH_LAUNCH_COMMAND: &str =
    "cd /root/workspace && echo $$ > /tmp/devvm-daemon-dsh.pid && exec dsh web";
const DSH_STOP_COMMAND: &str = r#"
pid_file=/tmp/devvm-daemon-dsh.pid
if [ -s "$pid_file" ]; then
    pid=$(cat "$pid_file")
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
        attempts=0
        while kill -0 "$pid" 2>/dev/null && [ "$attempts" -lt 100 ]; do
            sleep 0.05
            attempts=$((attempts + 1))
        done
        if kill -0 "$pid" 2>/dev/null; then
            kill -KILL "$pid" 2>/dev/null || true
            sleep 0.1
        fi
        if kill -0 "$pid" 2>/dev/null; then
            echo "DSH process $pid did not stop" >&2
            exit 1
        fi
    fi
    rm -f "$pid_file"
fi
"#;

type StopResultSender = oneshot::Sender<Result<(), String>>;
type StopRequestSender = oneshot::Sender<StopResultSender>;

#[derive(Debug)]
#[allow(dead_code)]
enum ProcessState {
    Starting { stop_tx: Option<StopRequestSender> },
    Running { stop_tx: Option<StopRequestSender> },
    Stopping,
    Stopped,
    Failed(String),
}

#[derive(Clone, Default)]
pub struct DshRuntimeManager {
    states: Arc<Mutex<HashMap<Uuid, ProcessState>>>,
}

impl DshRuntimeManager {
    pub fn new() -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_status(&self, project_id: Uuid) -> DshStatus {
        let states = self.states.lock().await;
        match states.get(&project_id) {
            Some(ProcessState::Starting { .. }) => DshStatus::Starting,
            Some(ProcessState::Running { .. }) => DshStatus::Running,
            Some(ProcessState::Stopping) => DshStatus::Stopping,
            Some(ProcessState::Failed(_)) => DshStatus::Failed,
            Some(ProcessState::Stopped) | None => DshStatus::Stopped,
        }
    }

    pub async fn launch_dsh(
        &self,
        config: &DaemonConfig,
        sync_manager: &SyncManager,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), String> {
        {
            let mut states = self.states.lock().await;
            match states.get(&project_id) {
                Some(ProcessState::Running { .. }) | Some(ProcessState::Starting { .. }) => {
                    return Ok(())
                }
                Some(ProcessState::Stopping) => {
                    return Err("DSH stop is still in progress".to_string())
                }
                _ => {}
            }
            states.insert(project_id, ProcessState::Starting { stop_tx: None });
        }

        if let Err(error) = self
            .prepare_launch(config, sync_manager, project_id, project_path)
            .await
        {
            self.states
                .lock()
                .await
                .insert(project_id, ProcessState::Stopped);
            return Err(error);
        }

        append_log(
            &config.log_dir,
            project_id,
            "daemon",
            "Launching DSH Runtime inside DevVM...",
        )
        .map_err(|error| error.to_string())?;

        let mut command = tokio::process::Command::new(&config.devvm_bin);
        command
            .arg("exec")
            .arg("/bin/bash")
            .arg("-c")
            .arg(DSH_LAUNCH_COMMAND)
            .current_dir(project_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let message = format!("Failed to spawn DSH command: {}", error);
                let _ = append_log(&config.log_dir, project_id, "daemon:error", &message);
                self.states
                    .lock()
                    .await
                    .insert(project_id, ProcessState::Failed(message.clone()));
                return Err(message);
            }
        };

        let (ready_tx, mut ready_rx) = oneshot::channel::<()>();
        let (stop_tx, mut stop_rx) = oneshot::channel::<StopResultSender>();
        let (startup_tx, startup_rx) = oneshot::channel::<Result<(), String>>();

        if let Some(stdout) = child.stdout.take() {
            let log_dir = config.log_dir.clone();
            tokio::spawn(async move {
                let mut ready_tx = Some(ready_tx);
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let ready = line.contains("dsh web: http://");
                    let _ = append_log(&log_dir, project_id, "dsh", &line);
                    if ready {
                        if let Some(tx) = ready_tx.take() {
                            let _ = tx.send(());
                        }
                    }
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let log_dir = config.log_dir.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = append_log(&log_dir, project_id, "dsh:err", &line);
                }
            });
        }

        self.states.lock().await.insert(
            project_id,
            ProcessState::Starting {
                stop_tx: Some(stop_tx),
            },
        );

        let config_clone = config.clone();
        let project_path = project_path.to_path_buf();
        let states = Arc::clone(&self.states);
        tokio::spawn(async move {
            tokio::select! {
                readiness = &mut ready_rx => {
                    match readiness {
                        Ok(()) => {
                            let mut process_states = states.lock().await;
                            let stop_tx = match process_states.get_mut(&project_id) {
                                Some(ProcessState::Starting { stop_tx }) => stop_tx.take(),
                                _ => None,
                            };
                            process_states.insert(project_id, ProcessState::Running { stop_tx });
                            drop(process_states);
                            let _ = startup_tx.send(Ok(()));

                            tokio::select! {
                                stop_result_tx = &mut stop_rx => {
                                    let result = stop_managed_process(
                                        &config_clone,
                                        &states,
                                        project_id,
                                        &project_path,
                                        &mut child,
                                    ).await;
                                    let _ = stop_result_tx.map(|tx| tx.send(result));
                                }
                                status = child.wait() => {
                                    record_unexpected_exit(
                                        &config_clone,
                                        &states,
                                        project_id,
                                        status,
                                    ).await;
                                }
                            }
                        }
                        Err(_) => {
                            let message = "DSH closed its output before reporting readiness".to_string();
                            let _ = child.kill().await;
                            states.lock().await.insert(project_id, ProcessState::Failed(message.clone()));
                            let _ = startup_tx.send(Err(message));
                        }
                    }
                }
                stop_result_tx = &mut stop_rx => {
                    let result = stop_managed_process(
                        &config_clone,
                        &states,
                        project_id,
                        &project_path,
                        &mut child,
                    ).await;
                    let _ = stop_result_tx.map(|tx| tx.send(result.clone()));
                    let _ = startup_tx.send(Err("DSH launch was stopped".to_string()));
                }
                status = child.wait() => {
                    let message = record_unexpected_exit(
                        &config_clone,
                        &states,
                        project_id,
                        status,
                    ).await;
                    let _ = startup_tx.send(Err(message));
                }
            }
        });

        startup_rx
            .await
            .map_err(|_| "DSH startup monitor exited unexpectedly".to_string())?
    }

    async fn prepare_launch(
        &self,
        config: &DaemonConfig,
        sync_manager: &SyncManager,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), String> {
        sync_manager
            .reconcile_startup(config, project_id, project_path)
            .await
            .map_err(|error| error.to_string())?;

        let vm_status = check_vm_status(config, project_path).await;
        if vm_status != crate::models::VmStatus::Running {
            append_log(
                &config.log_dir,
                project_id,
                "daemon",
                "DevVM is not running. Starting DevVM before launching DSH...",
            )
            .map_err(|error| error.to_string())?;
            run_vm_start(config, project_id, project_path).await?;
        }

        Ok(())
    }

    pub async fn stop_dsh(&self, config: &DaemonConfig, project_id: Uuid) -> Result<(), String> {
        let stop_tx = {
            let mut states = self.states.lock().await;
            let Some(state) = states.get_mut(&project_id) else {
                return Ok(());
            };

            let tx = match state {
                ProcessState::Starting { stop_tx } | ProcessState::Running { stop_tx } => stop_tx
                    .take()
                    .ok_or_else(|| "DSH lifecycle operation is already in progress".to_string())?,
                ProcessState::Stopping => return Err("DSH stop is already in progress".to_string()),
                ProcessState::Stopped | ProcessState::Failed(_) => return Ok(()),
            };
            *state = ProcessState::Stopping;
            tx
        };

        let _ = append_log(&config.log_dir, project_id, "daemon", "DSH stop requested");
        let (result_tx, result_rx) = oneshot::channel();
        stop_tx
            .send(result_tx)
            .map_err(|_| "DSH process monitor is unavailable".to_string())?;
        result_rx
            .await
            .map_err(|_| "DSH process monitor stopped before cleanup finished".to_string())?
    }

    pub async fn handle_vm_stopped(&self, config: &DaemonConfig, project_id: Uuid) {
        let _ = self.stop_dsh(config, project_id).await;
    }
}

async fn stop_dsh_inside_vm(
    config: &DaemonConfig,
    project_id: Uuid,
    project_path: &Path,
) -> Result<(), String> {
    let mut command = tokio::process::Command::new(&config.devvm_bin);
    command
        .arg("exec")
        .arg("/bin/bash")
        .arg("-c")
        .arg(DSH_STOP_COMMAND)
        .current_dir(project_path);

    let output = command
        .output()
        .await
        .map_err(|error| format!("Failed to stop DSH inside DevVM: {}", error))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.trim().is_empty() {
        let _ = append_log(&config.log_dir, project_id, "dsh:stop", stdout.trim());
    }
    if !stderr.trim().is_empty() {
        let _ = append_log(&config.log_dir, project_id, "dsh:stop:err", stderr.trim());
    }
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Stopping DSH inside DevVM exited with status {:?}: {}",
            output.status.code(),
            stderr.trim()
        ))
    }
}

async fn stop_managed_process(
    config: &DaemonConfig,
    states: &Arc<Mutex<HashMap<Uuid, ProcessState>>>,
    project_id: Uuid,
    project_path: &Path,
    child: &mut Child,
) -> Result<(), String> {
    let _ = append_log(
        &config.log_dir,
        project_id,
        "daemon",
        "Stopping DSH process...",
    );
    let result = stop_dsh_inside_vm(config, project_id, project_path).await;
    let _ = child.kill().await;
    let _ = child.wait().await;

    let next_state = match &result {
        Ok(()) => ProcessState::Stopped,
        Err(error) => ProcessState::Failed(error.clone()),
    };
    states.lock().await.insert(project_id, next_state);

    match &result {
        Ok(()) => {
            let _ = append_log(
                &config.log_dir,
                project_id,
                "daemon",
                "DSH process stopped.",
            );
        }
        Err(error) => {
            let _ = append_log(&config.log_dir, project_id, "daemon:error", error);
        }
    }
    result
}

async fn record_unexpected_exit(
    config: &DaemonConfig,
    states: &Arc<Mutex<HashMap<Uuid, ProcessState>>>,
    project_id: Uuid,
    status: std::io::Result<std::process::ExitStatus>,
) -> String {
    let message = match status {
        Ok(status) => format!("DSH process exited with status: {:?}", status.code()),
        Err(error) => format!("DSH process wait error: {}", error),
    };
    let _ = append_log(&config.log_dir, project_id, "daemon:error", &message);
    states
        .lock()
        .await
        .insert(project_id, ProcessState::Failed(message.clone()));
    message
}
