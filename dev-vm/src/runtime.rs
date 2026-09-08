use crate::config::DaemonConfig;
use crate::lifecycle::{command_output, LifecycleError, Operations};
use crate::logs::{append_log, append_log_logged};
use crate::models::{DshStatus, VmStatus};
use crate::runner::{
    check_vm_status, check_vm_status_with_cancel, log_command_failure, log_command_spawn_failure,
    run_vm_delete, run_vm_start, run_vm_stop,
};
use std::path::Path;
use std::process::Stdio;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Starts DSH detached inside the DevVM. Idempotent: an existing live pid short-circuits it
/// before any stale pid or token state is removed, so a relaunch never wipes the token the
/// running DSH emitted. Only an actual launch removes stale state and captures the new token.
/// `{project_id}` is substituted before the snippet runs. `echo $$` runs inside the inner
/// bash, which `exec`s DSH, so the pid file holds DSH's own pid and never the prefixer's.
const DSH_START_COMMAND: &str = r#"
pid_file=/tmp/devvm-daemon-dsh.pid
token_file=/tmp/devvm-daemon-dsh.token
if [ -s "$pid_file" ] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then exit 0; fi
log_dir=/devvm-root/.project-logs/{project_id}
install -d -m 0700 "$log_dir"
rm -f "$pid_file" "$token_file" "$log_dir/dsh.token"
setsid bash -c '
  echo $$ > /tmp/devvm-daemon-dsh.pid
  cd /root/workspace && devvm-sync-startup
  exec dsh web --no-open
' </dev/null 2>&1 | while IFS= read -r line; do
  case "$line" in
    *"dsh web:"*"token="*)
      token="${line##*token=}"
      token="${token%%[ &]*}"
      if [ -n "$token" ]; then
        printf '%s\n' "$token" > "$log_dir/dsh.token.tmp" && mv -f "$log_dir/dsh.token.tmp" "$log_dir/dsh.token"
        printf '%s\n' "$token" > "$token_file.tmp" && mv -f "$token_file.tmp" "$token_file"
      fi
      ;;
  esac
  printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)" "$line"
done >> "$log_dir/dsh.log" 2>&1 &
"#;
// Exit codes: 0 = ready, 1 = stopped, 2 = starting. The launch snippet clears stale
// tokens before spawning, then captures a fresh token only from DSH's ready URL.
const DSH_STATUS_COMMAND: &str = r#"
[ -s /tmp/devvm-daemon-dsh.pid ] && kill -0 "$(cat /tmp/devvm-daemon-dsh.pid)" 2>/dev/null || exit 1
[ -s /tmp/devvm-daemon-dsh.token ] || exit 2
"#;
const DSH_STOP_COMMAND: &str = r#"
pid_file=/tmp/devvm-daemon-dsh.pid
token_file=/tmp/devvm-daemon-dsh.token
log_dir=/devvm-root/.project-logs/{project_id}
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
fi
rm -f "$pid_file" "$token_file" "$log_dir/dsh.token"
"#;

#[derive(Clone, Copy)]
pub enum LifecycleAction {
    StartVm,
    StopVm,
    DeleteVm,
    LaunchDsh,
    StopDsh,
    RestartDsh,
}

impl LifecycleAction {
    pub fn message(self) -> &'static str {
        match self {
            Self::StartVm => "DevVM started",
            Self::StopVm => "DevVM stopped",
            Self::DeleteVm => "DevVM deleted",
            Self::LaunchDsh => "DSH launched",
            Self::StopDsh => "DSH stopped",
            Self::RestartDsh => "DSH restarted",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::StartVm => "VM start",
            Self::StopVm => "VM stop",
            Self::DeleteVm => "VM delete",
            Self::LaunchDsh => "DSH launch",
            Self::StopDsh => "DSH stop",
            Self::RestartDsh => "DSH restart",
        }
    }
}

/// The DevVM owns DSH state. This manager coordinates commands, never caches or
/// manufactures runtime status from an HTTP request's progress.
#[derive(Clone, Default)]
pub struct DshRuntimeManager {
    operations: Operations,
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
        let vm_status = check_vm_status(config, project_path).await;
        self.get_status_for_vm(config, project_id, project_path, vm_status)
            .await
    }

    pub(crate) async fn get_status_for_vm(
        &self,
        config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
        vm_status: VmStatus,
    ) -> DshStatus {
        // `devvm exec` would create and start a DevVM, so never probe a stopped or
        // unobservable VM. A failed VM probe is not evidence that DSH is stopped.
        match vm_status {
            VmStatus::Stopped => return DshStatus::Stopped,
            VmStatus::Running => {}
            _ => return DshStatus::Unknown,
        }
        let status = probe_dsh(config, project_path).await;
        if status == DshStatus::Stopped {
            // The DevVM owns DSH and its token file; when the guest probe finds no live DSH the
            // daemon drops its copy so no stale token ever authenticates a stopped runtime.
            let _ = self.remove_token(config, project_id);
        }
        status
    }

    pub fn get_token(&self, config: &DaemonConfig, project_id: Uuid) -> Option<String> {
        let path = crate::logs::dsh_token_path(&config.log_dir, project_id);
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Removes the daemon's runtime copy of the DSH launch token for a Project.
    fn remove_token(&self, config: &DaemonConfig, project_id: Uuid) -> std::io::Result<()> {
        std::fs::remove_file(crate::logs::dsh_token_path(&config.log_dir, project_id))
    }

    pub async fn execute(
        &self,
        config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
        action: LifecycleAction,
    ) -> Result<(), LifecycleError> {
        let manager = self.clone();
        let owned_config = config.clone();
        let path = project_path.to_owned();
        let stopping = matches!(
            action,
            LifecycleAction::StopVm | LifecycleAction::DeleteVm | LifecycleAction::StopDsh
        );
        self.operations
            .run(
                config,
                project_id,
                stopping,
                action.label(),
                move |cancel| async move {
                    let config = &owned_config;
                    match action {
                        LifecycleAction::StartVm => {
                            run_vm_start(config, project_id, &path, &cancel).await
                        }
                        LifecycleAction::LaunchDsh => {
                            manager.run_launch(config, project_id, &path, &cancel).await
                        }
                        LifecycleAction::StopDsh => {
                            manager.run_stop(config, project_id, &path, &cancel).await
                        }
                        LifecycleAction::RestartDsh => {
                            manager.run_stop(config, project_id, &path, &cancel).await?;
                            manager.run_launch(config, project_id, &path, &cancel).await
                        }
                        LifecycleAction::StopVm | LifecycleAction::DeleteVm => {
                            // A guest stop error must not prevent stopping the entire VM, but
                            // request cancellation must not proceed into another command.
                            if let Err(error) =
                                manager.run_stop(config, project_id, &path, &cancel).await
                            {
                                append_log_logged(
                                    &config.log_dir,
                                    project_id,
                                    "daemon:error",
                                    &error,
                                );
                            }
                            if matches!(action, LifecycleAction::StopVm) {
                                run_vm_stop(config, project_id, &path, &cancel).await
                            } else {
                                run_vm_delete(config, project_id, &path, &cancel).await
                            }
                        }
                    }
                },
            )
            .await
    }

    async fn run_launch(
        &self,
        config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        if check_vm_status_with_cancel(config, project_path, cancel).await != VmStatus::Running {
            append_log(
                &config.log_dir,
                project_id,
                "daemon",
                "DevVM is not running. Starting DevVM before launching DSH...",
            )
            .map_err(|error| error.to_string())?;
            run_vm_start(config, project_id, project_path, cancel).await?;
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
        // Stale pid and token state is removed by the snippet itself, only on an actual
        // launch; an idempotent relaunch must not wipe the token the running DSH emitted.
        run_guest_command(
            config,
            project_path,
            &DSH_START_COMMAND.replace("{project_id}", &project_id.to_string()),
            "Starting DSH inside DevVM",
            cancel,
        )
        .await
    }

    async fn run_stop(
        &self,
        config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        // A stopped DevVM has no DSH Runtime, and `devvm exec` would start the DevVM to look.
        match check_vm_status_with_cancel(config, project_path, cancel).await {
            VmStatus::Stopped => {
                let _ = self.remove_token(config, project_id);
                return Ok(());
            }
            VmStatus::Running => {}
            _ => return Err("Cannot determine VM status before stopping DSH".to_string()),
        }

        append_log_logged(&config.log_dir, project_id, "daemon", "DSH stop requested");
        let stop_command = DSH_STOP_COMMAND.replace("{project_id}", &project_id.to_string());
        let result = run_guest_command(
            config,
            project_path,
            &stop_command,
            "Stopping DSH inside DevVM",
            cancel,
        )
        .await;
        if result.is_ok() {
            let _ = self.remove_token(config, project_id);
        }
        result
    }
}

/// The guest distinguishes a live runtime still starting from one that emitted its ready URL.
async fn probe_dsh(config: &DaemonConfig, project_path: &Path) -> DshStatus {
    let command = guest_command(config, project_path, DSH_STATUS_COMMAND);
    match command_output(command, &CancellationToken::new()).await {
        Ok(output) if output.status.success() => DshStatus::Running,
        Ok(output) if output.status.code() == Some(2) => DshStatus::Starting,
        Ok(output) if output.status.code() == Some(1) => DshStatus::Stopped,
        Ok(_) => DshStatus::Unknown,
        Err(error) => {
            log_command_spawn_failure(
                &config.devvm_bin.display().to_string(),
                &guest_args(DSH_STATUS_COMMAND),
                &error,
            );
            DshStatus::Unknown
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
    cancel: &CancellationToken,
) -> Result<(), String> {
    let program = config.devvm_bin.display().to_string();
    let args = guest_args(snippet);
    let output = match command_output(guest_command(config, project_path, snippet), cancel).await {
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
    use super::{DSH_START_COMMAND, DSH_STATUS_COMMAND, DSH_STOP_COMMAND};

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
    fn test_status_command_waits_for_token_from_live_runtime() {
        let guest = tempfile::tempdir().unwrap();
        let pid_file = guest.path().join("dsh.pid");
        let token_file = guest.path().join("dsh.token");
        let command = DSH_STATUS_COMMAND
            .replace("/tmp/devvm-daemon-dsh.pid", pid_file.to_str().unwrap())
            .replace("/tmp/devvm-daemon-dsh.token", token_file.to_str().unwrap());
        let probe = || {
            std::process::Command::new("bash")
                .args(["-c", &command])
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap()
                .status
                .code()
        };

        assert_eq!(probe(), Some(1), "no process is stopped");
        std::fs::write(&pid_file, std::process::id().to_string()).unwrap();
        assert_eq!(probe(), Some(2), "live process without URL is starting");
        std::fs::write(&token_file, "").unwrap();
        assert_eq!(probe(), Some(2), "empty token is not readiness");
        std::fs::write(&token_file, "test-launch-token").unwrap();
        assert_eq!(probe(), Some(0), "live process with token is running");
        std::fs::remove_file(&pid_file).unwrap();
        assert_eq!(
            probe(),
            Some(1),
            "stale token cannot make a runtime running"
        );
    }

    #[test]
    fn test_start_command_launches_dsh_with_no_open() {
        assert!(
            DSH_START_COMMAND.contains("exec dsh web --no-open"),
            "start command must launch DSH with --no-open to preserve non-interactive execution"
        );
    }

    #[test]
    fn test_start_command_captures_token_into_runtime_files() {
        assert!(
            DSH_START_COMMAND.contains(r#"case "$line" in"#),
            "start command must inspect lines for token"
        );
        assert!(
            DSH_START_COMMAND.contains(r#"*"dsh web:"*"token="*"#),
            "start command must match dsh web startup line"
        );
        assert!(
            DSH_START_COMMAND.contains("dsh.token"),
            "start command must write dsh.token in log_dir"
        );
    }

    #[test]
    fn test_start_command_clears_stale_state_only_after_the_live_check_and_before_launch() {
        let kill_at = DSH_START_COMMAND
            .find(r#"kill -0 "$(cat "$pid_file")""#)
            .expect("start command must check the live pid");
        let rm_at = DSH_START_COMMAND
            .find(r#"rm -f "$pid_file" "$token_file" "$log_dir/dsh.token""#)
            .expect("start command must clear stale pid and token state");
        let launch_at = DSH_START_COMMAND
            .find("setsid bash -c")
            .expect("start command must launch DSH");
        assert!(
            kill_at < rm_at,
            "the live check must run first: a redundant launch must never clear the current token"
        );
        assert!(
            rm_at < launch_at,
            "stale token state must be cleared before a new launch, never served by it"
        );
    }

    #[test]
    fn test_start_command_parses_the_token_at_its_line_end_then_stops() {
        assert!(
            DSH_START_COMMAND.contains("token=\"${line##*token=}\""),
            "the token must be cut from the startup line"
        );
        assert!(
            DSH_START_COMMAND.contains("token=\"${token%%[ &]*}\""),
            "the token must be cut at the first space or ampersand"
        );
    }

    #[test]
    fn test_start_command_replaces_the_previous_token_atomically() {
        assert!(
            DSH_START_COMMAND.contains("mv -f \"$log_dir/dsh.token.tmp\" \"$log_dir/dsh.token\""),
            "the captured token must atomically replace any previous token"
        );
    }

    #[test]
    fn test_stop_command_removes_pid_and_token_state() {
        assert!(
            DSH_STOP_COMMAND.contains(r#"rm -f "$pid_file" "$token_file" "$log_dir/dsh.token""#),
            "stop command must remove the pid and token state"
        );
    }
}
