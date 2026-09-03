use crate::config::DaemonConfig;
use crate::logs::append_log_logged;
use crate::models::VmStatus;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
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
    let program = config.devvm_bin.display().to_string();
    let args = vec!["status".to_string()];
    let mut cmd = Command::new(&config.devvm_bin);
    cmd.arg("status")
        .current_dir(project_path)
        .stdin(Stdio::null());

    match cmd.output().await {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
                if stdout.contains("running") || stdout.is_empty() {
                    VmStatus::Running
                } else if stdout.contains("stopped") {
                    VmStatus::Stopped
                } else {
                    tracing::warn!(
                        program,
                        args = ?args,
                        stdout = %stdout,
                        "devvm status reported neither running nor stopped"
                    );
                    VmStatus::Running
                }
            } else {
                log_command_failure(&program, &args, &output);
                VmStatus::Stopped
            }
        }
        Err(e) => {
            log_command_spawn_failure(&program, &args, &e);
            VmStatus::Stopped
        }
    }
}

async fn run_devvm_command(
    config: &DaemonConfig,
    project_id: Uuid,
    project_path: &Path,
    subcmd: &str,
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

    match cmd.output().await {
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
) -> Result<(), String> {
    run_devvm_command(config, project_id, project_path, "start").await
}

pub async fn run_vm_stop(
    config: &DaemonConfig,
    project_id: Uuid,
    project_path: &Path,
) -> Result<(), String> {
    run_devvm_command(config, project_id, project_path, "stop").await
}

pub async fn run_vm_delete(
    config: &DaemonConfig,
    project_id: Uuid,
    project_path: &Path,
) -> Result<(), String> {
    run_devvm_command(config, project_id, project_path, "rm").await
}
