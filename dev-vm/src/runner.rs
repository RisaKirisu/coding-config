use crate::config::DaemonConfig;
use crate::lifecycle::command_output;
use crate::logs::append_log_logged;
use crate::models::VmStatus;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub fn log_command_failure(program: &str, args: &[String], output: &std::process::Output) {
    tracing::error!(
        program,
        args = ?args,
        exit_code = ?output.status.code(),
        stdout = %String::from_utf8_lossy(&output.stdout),
        stderr = %String::from_utf8_lossy(&output.stderr),
        "command failed"
    );
}

pub fn log_command_spawn_failure(program: &str, args: &[String], error: &std::io::Error) {
    tracing::error!(
        program,
        args = ?args,
        error = %error,
        "command could not be executed"
    );
}

pub async fn check_vm_status(config: &DaemonConfig, project_path: &Path) -> VmStatus {
    check_vm_status_with_cancel(config, project_path, &CancellationToken::new()).await
}

pub(crate) async fn check_vm_status_with_cancel(
    config: &DaemonConfig,
    project_path: &Path,
    cancel: &CancellationToken,
) -> VmStatus {
    let program = config.devvm_bin.display().to_string();
    let args = vec!["status".to_string()];
    let mut cmd = Command::new(&config.devvm_bin);
    cmd.arg("status")
        .current_dir(project_path)
        .stdin(Stdio::null());

    match command_output(cmd, cancel).await {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let line = stdout.trim();

                if line.is_empty() {
                    VmStatus::Stopped
                } else {
                    let state = line
                        .strip_prefix("Machine '")
                        .and_then(|rest| rest.rsplit_once("': "))
                        .map(|(_, status)| status.split_whitespace().next().unwrap_or(""));

                    match state {
                        Some("running") => VmStatus::Running,
                        Some("created" | "stopped") => VmStatus::Stopped,
                        Some("failed" | "unreachable" | "frozen") => VmStatus::Failed,
                        _ => {
                            tracing::warn!(
                                program,
                                args = ?args,
                                stdout = %stdout,
                                "unexpected devvm status output"
                            );
                            VmStatus::Failed
                        }
                    }
                }
            } else {
                log_command_failure(&program, &args, &output);
                VmStatus::Stopped
            }
        }
        Err(e) => {
            log_command_spawn_failure(&program, &args, &e);
            VmStatus::Failed
        }
    }
}

async fn run_devvm_command(
    config: &DaemonConfig,
    project_id: Uuid,
    project_path: &Path,
    subcmd: &str,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let cmd_desc = format!("devvm {}", subcmd);
    append_log_logged(
        &config.log_dir,
        project_id,
        "daemon",
        &format!("Invoking `{}`", cmd_desc),
    );
    let program = config.devvm_bin.display().to_string();
    let args = vec![subcmd.to_string()];
    let mut cmd = Command::new(&config.devvm_bin);
    cmd.arg(subcmd)
        .current_dir(project_path)
        .stdin(Stdio::null());

    match command_output(cmd, cancel).await {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stdout.is_empty() {
                append_log_logged(&config.log_dir, project_id, "devvm", &stdout);
            }
            if !stderr.is_empty() {
                append_log_logged(&config.log_dir, project_id, "devvm:err", &stderr);
            }

            if output.status.success() {
                append_log_logged(
                    &config.log_dir,
                    project_id,
                    "daemon",
                    &format!("`{}` succeeded", cmd_desc),
                );
                Ok(())
            } else {
                log_command_failure(&program, &args, &output);
                let err_msg = format!(
                    "{} exited with status {:?}: {}",
                    cmd_desc,
                    output.status.code(),
                    stderr.trim()
                );
                Err(err_msg)
            }
        }
        Err(e) => {
            log_command_spawn_failure(&program, &args, &e);
            Err(format!("Failed to execute `{}`: {}", cmd_desc, e))
        }
    }
}

pub async fn run_vm_start(
    config: &DaemonConfig,
    project_id: Uuid,
    project_path: &Path,
    cancel: &CancellationToken,
) -> Result<(), String> {
    run_devvm_command(config, project_id, project_path, "start", cancel).await
}

pub async fn run_vm_stop(
    config: &DaemonConfig,
    project_id: Uuid,
    project_path: &Path,
    cancel: &CancellationToken,
) -> Result<(), String> {
    run_devvm_command(config, project_id, project_path, "stop", cancel).await
}

pub async fn run_vm_delete(
    config: &DaemonConfig,
    project_id: Uuid,
    project_path: &Path,
    cancel: &CancellationToken,
) -> Result<(), String> {
    run_devvm_command(config, project_id, project_path, "rm", cancel).await
}
