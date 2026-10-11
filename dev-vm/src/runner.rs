//! External commands the daemon runs. Every `devvm` and `ssh` invocation is described as a
//! [`CommandSpec`] and executed through a [`CommandRunner`]; production uses
//! [`ProcessCommandRunner`], which spawns the real process.
use crate::config::DaemonConfig;
use crate::lifecycle::command_output;
use crate::logs::append_log_logged;
use crate::models::{ProjectMemory, VmStatus};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Output;
use std::sync::Arc;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// One external command: the program, its arguments, and its working directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub current_dir: Option<PathBuf>,
}

pub type CommandFuture = Pin<Box<dyn Future<Output = io::Result<Output>> + Send>>;

/// Runs a command to completion and captures its output. Cancelling `cancel` must stop the
/// command and resolve with `ErrorKind::Interrupted` once its cleanup has finished.
pub trait CommandRunner: Send + Sync {
    fn run(&self, command: CommandSpec, cancel: CancellationToken) -> CommandFuture;
}

/// Spawns the real process in its own process group; see [`command_output`].
pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&self, command: CommandSpec, cancel: CancellationToken) -> CommandFuture {
        Box::pin(async move {
            let mut process = Command::new(&command.program);
            process.args(&command.args);
            if let Some(dir) = &command.current_dir {
                process.current_dir(dir);
            }
            command_output(process, &cancel).await
        })
    }
}

pub fn log_command_failure(program: &str, args: &[String], output: &Output) {
    tracing::error!(
        program,
        args = ?args,
        exit_code = ?output.status.code(),
        stdout = %String::from_utf8_lossy(&output.stdout),
        stderr = %String::from_utf8_lossy(&output.stderr),
        "command failed"
    );
}

pub fn log_command_spawn_failure(program: &str, args: &[String], error: &io::Error) {
    tracing::error!(
        program,
        args = ?args,
        error = %error,
        "command could not be executed"
    );
}

/// The `devvm` CLI, invoked from a Project directory.
#[derive(Clone)]
pub struct Devvm {
    bin: PathBuf,
    runner: Arc<dyn CommandRunner>,
}

impl Devvm {
    pub fn new(bin: PathBuf, runner: Arc<dyn CommandRunner>) -> Self {
        Self { bin, runner }
    }

    pub fn program(&self) -> String {
        self.bin.display().to_string()
    }

    pub async fn output(
        &self,
        project_path: &Path,
        args: Vec<String>,
        cancel: &CancellationToken,
    ) -> io::Result<Output> {
        let command = CommandSpec {
            program: self.bin.clone(),
            args,
            current_dir: Some(project_path.to_owned()),
        };
        self.runner.run(command, cancel.clone()).await
    }

    pub async fn status(&self, project_path: &Path, cancel: &CancellationToken) -> VmStatus {
        let args = vec!["status".to_string()];
        match self.output(project_path, args.clone(), cancel).await {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                parse_vm_status(&stdout).unwrap_or_else(|| {
                    tracing::warn!(
                        program = %self.program(),
                        args = ?args,
                        stdout = %stdout,
                        "unexpected devvm status output"
                    );
                    VmStatus::Failed
                })
            }
            Ok(output) => {
                log_command_failure(&self.program(), &args, &output);
                VmStatus::Stopped
            }
            Err(error) => {
                log_command_spawn_failure(&self.program(), &args, &error);
                VmStatus::Failed
            }
        }
    }

    pub async fn memory(&self, project_path: &Path) -> Option<ProjectMemory> {
        let output = self
            .output(
                project_path,
                vec!["memory".to_string()],
                &CancellationToken::new(),
            )
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_vm_memory(&String::from_utf8_lossy(&output.stdout))
    }

    /// Runs a lifecycle subcommand (`start`, `stop`, `rm`) and records it in the Project log.
    pub async fn run_lifecycle(
        &self,
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
        let args = vec![subcmd.to_string()];
        match self.output(project_path, args.clone(), cancel).await {
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
                    log_command_failure(&self.program(), &args, &output);
                    Err(format!(
                        "{} exited with status {:?}: {}",
                        cmd_desc,
                        output.status.code(),
                        stderr.trim()
                    ))
                }
            }
            Err(e) => {
                log_command_spawn_failure(&self.program(), &args, &e);
                Err(format!("Failed to execute `{}`: {}", cmd_desc, e))
            }
        }
    }
}

/// Maps `devvm status` output (`Machine '<name>': <state>`, or nothing for an absent
/// machine) to a VM status. Returns `None` for unrecognised output.
pub fn parse_vm_status(stdout: &str) -> Option<VmStatus> {
    let line = stdout.trim();
    if line.is_empty() {
        return Some(VmStatus::Stopped);
    }
    let state = line
        .strip_prefix("Machine '")
        .and_then(|rest| rest.rsplit_once("': "))
        .map(|(_, status)| status.split_whitespace().next().unwrap_or(""));
    match state? {
        "running" => Some(VmStatus::Running),
        "created" | "stopped" => Some(VmStatus::Stopped),
        "failed" | "unreachable" | "frozen" => Some(VmStatus::Failed),
        _ => None,
    }
}

#[derive(serde::Deserialize)]
struct RawMemoryOutput {
    #[serde(default)]
    host_bytes: Option<u64>,
    #[serde(default)]
    guest_bytes: Option<u64>,
    #[serde(default)]
    limit_bytes: Option<u64>,
}

/// Parses the JSON object printed by `devvm memory`.
pub fn parse_vm_memory(stdout: &str) -> Option<ProjectMemory> {
    let raw: RawMemoryOutput = serde_json::from_str(stdout.trim()).ok()?;
    if raw.host_bytes.is_none() && raw.guest_bytes.is_none() && raw.limit_bytes.is_none() {
        return None;
    }
    let formatted =
        crate::models::format_memory_display(raw.host_bytes, raw.guest_bytes, raw.limit_bytes);
    Some(ProjectMemory {
        host_bytes: raw.host_bytes,
        guest_bytes: raw.guest_bytes,
        limit_bytes: raw.limit_bytes,
        formatted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vm_status_maps_every_devvm_state() {
        assert_eq!(parse_vm_status(""), Some(VmStatus::Stopped));
        for (state, expected) in [
            ("running", VmStatus::Running),
            ("stopped", VmStatus::Stopped),
            ("created", VmStatus::Stopped),
            ("failed", VmStatus::Failed),
            ("unreachable", VmStatus::Failed),
            ("frozen", VmStatus::Failed),
        ] {
            assert_eq!(
                parse_vm_status(&format!("Machine 'dev-app-1234abcd': {state}\n")),
                Some(expected)
            );
        }
        assert_eq!(
            parse_vm_status("Machine 'dev-app-1234abcd': paused\n"),
            None
        );
        assert_eq!(parse_vm_status("garbage"), None);
    }

    #[test]
    fn parse_vm_memory_reads_devvm_memory_json() {
        let memory = parse_vm_memory(
            r#"{"host_bytes":1073741824,"guest_bytes":536870912,"limit_bytes":8589934592}"#,
        )
        .unwrap();
        assert_eq!(memory.host_bytes, Some(1073741824));
        assert_eq!(memory.guest_bytes, Some(536870912));
        assert_eq!(memory.limit_bytes, Some(8589934592));
        assert_eq!(
            parse_vm_memory(r#"{"host_bytes":null,"guest_bytes":null,"limit_bytes":null}"#),
            None
        );
        assert_eq!(parse_vm_memory("not json"), None);
    }
}
