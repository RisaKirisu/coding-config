use crate::config::DaemonConfig;
use crate::logs::{append_log, append_log_logged};
use crate::models::{DshStatus, VmStatus};
use crate::runner::{
    check_vm_status, log_command_failure, log_command_spawn_failure, run_vm_start,
};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uuid::Uuid;

/// Starts DSH detached inside the DevVM. Idempotent: an existing live pid short-circuits it.
/// `{project_id}` is substituted before the snippet runs. `echo $$` runs inside the inner
/// bash, which `exec`s DSH, so the pid file holds DSH's own pid and never the prefixer's.
const DSH_START_COMMAND: &str = r#"
pid_file=/tmp/devvm-daemon-dsh.pid
if [ -s "$pid_file" ] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then exit 0; fi
log_dir=/devvm-root/.project-logs/{project_id}
install -d -m 0700 "$log_dir"
setsid bash -c '
  echo $$ > /tmp/devvm-daemon-dsh.pid
  cd /root/workspace && devvm-sync-startup
  exec dsh web
' </dev/null 2>&1 | while IFS= read -r line; do printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)" "$line"; done >> "$log_dir/dsh.log" 2>&1 &
"#;
const DSH_STATUS_COMMAND: &str =
    r#"[ -s /tmp/devvm-daemon-dsh.pid ] && kill -0 "$(cat /tmp/devvm-daemon-dsh.pid)""#;
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

const STATUS_CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
enum InFlight {
    Starting,
    Stopping,
}

/// The DevVM owns the DSH Runtime; the daemon holds no child process, only the lifecycle
/// operation it is currently running and a short-lived cache of the last guest probe.
#[derive(Clone, Default)]
pub struct DshRuntimeManager {
    in_flight: Arc<Mutex<HashMap<Uuid, InFlight>>>,
    status_cache: Arc<Mutex<HashMap<Uuid, (Instant, DshStatus)>>>,
}

impl DshRuntimeManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_status(
        &self,
        config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> DshStatus {
        if let Some(operation) = self.in_flight.lock().await.get(&project_id) {
            return match operation {
                InFlight::Starting => DshStatus::Starting,
                InFlight::Stopping => DshStatus::Stopping,
            };
        }

        // `devvm exec` would create and start a DevVM, so it must not be probed while stopped.
        if check_vm_status(config, project_path).await != VmStatus::Running {
            return DshStatus::Stopped;
        }

        if let Some((probed_at, status)) = self.status_cache.lock().await.get(&project_id) {
            if probed_at.elapsed() < STATUS_CACHE_TTL {
                return *status;
            }
        }

        let status = probe_dsh(config, project_path).await;
        self.status_cache
            .lock()
            .await
            .insert(project_id, (Instant::now(), status));
        status
    }

    pub async fn launch_dsh(
        &self,
        config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), String> {
        self.begin(project_id, InFlight::Starting).await?;
        let result = self.run_launch(config, project_id, project_path).await;
        self.finish(project_id).await;
        result
    }

    async fn run_launch(
        &self,
        config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), String> {
        if check_vm_status(config, project_path).await != VmStatus::Running {
            append_log(
                &config.log_dir,
                project_id,
                "daemon",
                "DevVM is not running. Starting DevVM before launching DSH...",
            )
            .map_err(|error| error.to_string())?;
            run_vm_start(config, project_id, project_path).await?;
        }

        append_log(
            &config.log_dir,
            project_id,
            "daemon",
            "Launching DSH Runtime inside DevVM...",
        )
        .map_err(|error| error.to_string())?;

        // Invariant (ADR 0004): the guest snippet exits early when the pid file names a live
        // DSH, so a second launch starts no second process and reruns no reconciliation.
        run_guest_command(
            config,
            project_path,
            &DSH_START_COMMAND.replace("{project_id}", &project_id.to_string()),
            "Starting DSH inside DevVM",
        )
        .await
    }

    pub async fn stop_dsh(
        &self,
        config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), String> {
        // A stopped DevVM has no DSH Runtime, and `devvm exec` would start the DevVM to look.
        if check_vm_status(config, project_path).await != VmStatus::Running {
            self.status_cache.lock().await.remove(&project_id);
            return Ok(());
        }

        self.begin(project_id, InFlight::Stopping).await?;
        append_log_logged(&config.log_dir, project_id, "daemon", "DSH stop requested");
        let result = run_guest_command(
            config,
            project_path,
            DSH_STOP_COMMAND,
            "Stopping DSH inside DevVM",
        )
        .await;
        self.finish(project_id).await;
        result
    }

    async fn begin(&self, project_id: Uuid, operation: InFlight) -> Result<(), String> {
        let mut in_flight = self.in_flight.lock().await;
        if in_flight.contains_key(&project_id) {
            return Err("A DSH lifecycle operation is already in progress".to_string());
        }
        in_flight.insert(project_id, operation);
        Ok(())
    }

    async fn finish(&self, project_id: Uuid) {
        self.in_flight.lock().await.remove(&project_id);
        self.status_cache.lock().await.remove(&project_id);
    }

    pub async fn handle_vm_stopped(
        &self,
        config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
    ) {
        if let Err(e) = self.stop_dsh(config, project_id, project_path).await {
            tracing::error!(project = ?project_id, error = %e, "stopping DSH after DevVM stop failed");
        }
    }
}

/// A non-zero exit means no live pid file, which is an ordinary stopped DSH, not an error.
async fn probe_dsh(config: &DaemonConfig, project_path: &Path) -> DshStatus {
    let mut command = guest_command(config, project_path, DSH_STATUS_COMMAND);
    match command.output().await {
        Ok(output) if output.status.success() => DshStatus::Running,
        Ok(_) => DshStatus::Stopped,
        Err(error) => {
            log_command_spawn_failure(
                &config.devvm_bin.display().to_string(),
                &guest_args(DSH_STATUS_COMMAND),
                &error,
            );
            DshStatus::Stopped
        }
    }
}

fn guest_args(snippet: &str) -> Vec<String> {
    vec![
        "exec".to_string(),
        "/bin/bash".to_string(),
        "-c".to_string(),
        snippet.to_string(),
    ]
}

fn guest_command(
    config: &DaemonConfig,
    project_path: &Path,
    snippet: &str,
) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(&config.devvm_bin);
    command
        .arg("exec")
        .arg("/bin/bash")
        .arg("-c")
        .arg(snippet)
        .current_dir(project_path)
        .stdin(Stdio::null());
    command
}

async fn run_guest_command(
    config: &DaemonConfig,
    project_path: &Path,
    snippet: &str,
    context: &str,
) -> Result<(), String> {
    let program = config.devvm_bin.display().to_string();
    let args = guest_args(snippet);
    let output = match guest_command(config, project_path, snippet).output().await {
        Ok(output) => output,
        Err(error) => {
            log_command_spawn_failure(&program, &args, &error);
            return Err(format!("{}: {}", context, error));
        }
    };

    if output.status.success() {
        return Ok(());
    }

    log_command_failure(&program, &args, &output);
    Err(format!(
        "{} exited with status {:?}: {}",
        context,
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::{DSH_START_COMMAND, DSH_STATUS_COMMAND};

    #[test]
    fn test_start_command_runs_startup_script_before_dsh_web() {
        let script_at = DSH_START_COMMAND
            .find("devvm-sync-startup")
            .expect("start command must run devvm-sync-startup");
        let dsh_at = DSH_START_COMMAND
            .find("exec dsh web")
            .expect("start command must exec dsh web");
        assert!(script_at < dsh_at);
    }

    #[test]
    fn test_start_command_records_the_pid_of_dsh_itself() {
        let pid_write_at = DSH_START_COMMAND
            .find("echo $$ > /tmp/devvm-daemon-dsh.pid")
            .expect("the inner bash must record its own pid, which becomes DSH's after exec");
        let prefixer_at = DSH_START_COMMAND
            .find("while IFS= read -r line")
            .expect("start command must pipe DSH output through the timestamp prefixer");
        assert!(
            pid_write_at < prefixer_at,
            "the pid must be written inside the process that execs DSH, not in the prefixer"
        );
    }

    #[test]
    fn test_start_command_substitutes_the_project_id_into_the_log_dir() {
        let rendered = DSH_START_COMMAND.replace("{project_id}", "abc-123");
        assert!(rendered.contains("log_dir=/devvm-root/.project-logs/abc-123"));
        assert!(!rendered.contains("{project_id}"));
    }

    #[test]
    fn test_status_command_checks_the_guest_pid_file() {
        assert_eq!(
            DSH_STATUS_COMMAND,
            r#"[ -s /tmp/devvm-daemon-dsh.pid ] && kill -0 "$(cat /tmp/devvm-daemon-dsh.pid)""#
        );
    }
}
