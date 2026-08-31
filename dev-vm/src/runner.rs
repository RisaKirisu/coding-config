use crate::config::DaemonConfig;
use crate::logs::append_log;
use crate::models::VmStatus;
use std::path::Path;
use tokio::process::Command;
use uuid::Uuid;

pub async fn check_vm_status(config: &DaemonConfig, project_path: &Path) -> VmStatus {
    let mut cmd = Command::new(&config.devvm_bin);
    cmd.arg("status").current_dir(project_path);

    match cmd.output().await {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
                if stdout.contains("running") || stdout.is_empty() {
                    VmStatus::Running
                } else if stdout.contains("stopped") {
                    VmStatus::Stopped
                } else {
                    VmStatus::Running
                }
            } else {
                VmStatus::Stopped
            }
        }
        Err(_) => VmStatus::Stopped,
    }
}

async fn run_devvm_command(
    config: &DaemonConfig,
    project_id: Uuid,
    project_path: &Path,
    subcmd: &str,
) -> Result<(), String> {
    let cmd_desc = format!("devvm {}", subcmd);
    let _ = append_log(
        &config.log_dir,
        project_id,
        "daemon",
        &format!("Invoking `{}`", cmd_desc),
    );
    let mut cmd = Command::new(&config.devvm_bin);
    cmd.arg(subcmd).current_dir(project_path);

    match cmd.output().await {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stdout.is_empty() {
                let _ = append_log(&config.log_dir, project_id, "devvm", &stdout);
            }
            if !stderr.is_empty() {
                let _ = append_log(&config.log_dir, project_id, "devvm:err", &stderr);
            }

            if output.status.success() {
                let _ = append_log(
                    &config.log_dir,
                    project_id,
                    "daemon",
                    &format!("`{}` succeeded", cmd_desc),
                );
                Ok(())
            } else {
                let err_msg = format!(
                    "{} exited with status {:?}: {}",
                    cmd_desc,
                    output.status.code(),
                    stderr.trim()
                );
                let _ = append_log(&config.log_dir, project_id, "daemon:error", &err_msg);
                Err(err_msg)
            }
        }
        Err(e) => {
            let err_msg = format!("Failed to execute `{}`: {}", cmd_desc, e);
            let _ = append_log(&config.log_dir, project_id, "daemon:error", &err_msg);
            Err(err_msg)
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
