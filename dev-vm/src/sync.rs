use crate::config::DaemonConfig;
use crate::logs::append_log;
use crate::models::SyncStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncConfig {
    pub ssh_user: String,
    pub ssh_host: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    pub ssh_key_path: PathBuf,
    #[serde(default = "default_remote_sync_root")]
    pub remote_sync_root: String,
}

fn default_ssh_port() -> u16 {
    22
}

fn default_remote_sync_root() -> String {
    "/var/lib/devvm-sync".to_string()
}

pub fn load_sync_config(path: &Path) -> Result<Option<SyncConfig>, io::Error> {
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(path)?;
    if data.trim().is_empty() {
        return Ok(None);
    }
    let config: SyncConfig = serde_json::from_str(&data).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse sync config at {}: {}", path.display(), e),
        )
    })?;
    Ok(Some(config))
}

pub fn save_sync_config(path: &Path, config: &SyncConfig) -> Result<(), io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json_bytes = serde_json::to_vec_pretty(config).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Serialization error: {}", e),
        )
    })?;

    let tmp_path = path.with_extension(format!("tmp.{}", Uuid::new_v4()));
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(&json_bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp_path, path)?;
    Ok(())
}

pub fn is_local_state_dirty(local_dsh_path: &Path) -> bool {
    local_dsh_path.join(".sync-dirty").exists()
}

pub fn mark_local_state_dirty(local_dsh_path: &Path) -> io::Result<()> {
    if !local_dsh_path.exists() {
        fs::create_dir_all(local_dsh_path)?;
    }
    let dirty_file = local_dsh_path.join(".sync-dirty");
    fs::write(dirty_file, "1\n")?;
    Ok(())
}

pub fn mark_local_state_clean(local_dsh_path: &Path) -> io::Result<()> {
    let dirty_file = local_dsh_path.join(".sync-dirty");
    if dirty_file.exists() {
        fs::remove_file(dirty_file)?;
    }
    Ok(())
}

fn directory_contains_file(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };

    entries.flatten().any(|entry| match entry.file_type() {
        Ok(file_type) if file_type.is_file() => true,
        Ok(file_type) if file_type.is_dir() => directory_contains_file(&entry.path()),
        _ => false,
    })
}

pub fn check_local_portable_state_exists(local_dsh_path: &Path) -> bool {
    directory_contains_file(&local_dsh_path.join("sessions"))
        || local_dsh_path.join("storages/workspace.json").is_file()
        || local_dsh_path
            .join("storages/message_feedback.json")
            .is_file()
        || directory_contains_file(&local_dsh_path.join("attachments/v1/objects"))
}

pub fn resolve_host_ssh_key_path(guest_or_host_path: &Path) -> PathBuf {
    if guest_or_host_path.is_file() {
        return guest_or_host_path.to_path_buf();
    }
    let path_str = guest_or_host_path.to_string_lossy();
    if let Some(subpath) = path_str.strip_prefix("/root/") {
        let devvm_root = std::env::var("DEVVM_ROOT")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("DEVVM_HOME").map(|h| PathBuf::from(h).join("root")))
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|h| h.join("coding-config/dev-vm/root"))
                    .unwrap_or_else(|| PathBuf::from("/root/.local/share/devvm/root"))
            });
        let in_devvm_root = devvm_root.join(subpath);
        if in_devvm_root.is_file() {
            return in_devvm_root;
        }
        if let Some(home) = dirs::home_dir() {
            let in_home = home.join(subpath);
            if in_home.is_file() {
                return in_home;
            }
        }
    }
    guest_or_host_path.to_path_buf()
}

pub fn provision_sync_setup(
    daemon_config_path: &Path,
    sync_config: &SyncConfig,
) -> Result<SyncConfig, io::Error> {
    save_sync_config(daemon_config_path, sync_config)?;

    let devvm_root = std::env::var("DEVVM_ROOT")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("DEVVM_HOME").map(|h| PathBuf::from(h).join("root")))
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join("coding-config/dev-vm/root"))
                .unwrap_or_else(|| PathBuf::from("/root/.local/share/devvm/root"))
        });

    let shared_config_dir = devvm_root.join(".config/devvm");
    fs::create_dir_all(&shared_config_dir)?;

    let host_key = resolve_host_ssh_key_path(&sync_config.ssh_key_path);
    let guest_key_path = if host_key.is_file() {
        let shared_ssh_dir = devvm_root.join(".ssh");
        fs::create_dir_all(&shared_ssh_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&shared_ssh_dir, fs::Permissions::from_mode(0o700))?;
        }
        let key_name = host_key.file_name().unwrap_or_default();
        let target_key_path = shared_ssh_dir.join(key_name);
        if host_key != target_key_path {
            fs::copy(&host_key, &target_key_path)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target_key_path, fs::Permissions::from_mode(0o600))?;
        }
        let key_str = key_name.to_str().unwrap_or("id_ed25519");
        PathBuf::from(format!("/root/.ssh/{}", key_str))
    } else {
        sync_config.ssh_key_path.clone()
    };

    let mut guest_config = sync_config.clone();
    guest_config.ssh_key_path = guest_key_path;
    let shared_config_path = shared_config_dir.join("sync.json");
    save_sync_config(&shared_config_path, &guest_config)?;

    Ok(guest_config)
}

#[derive(Debug)]
pub enum SyncError {
    NotConfigured,
    ConfirmationRequired,
    IoError(io::Error),
    ConnectionFailed(String),
    PushFailed(String),
    PullFailed(String),
    DeletionFailed(String),
    StartupBlocked(String),
    ConfigError(String),
    Other(String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::NotConfigured => write!(f, "Sync is not configured"),
            SyncError::ConfirmationRequired => {
                write!(f, "Confirmation required: confirmed must be true")
            }
            SyncError::IoError(e) => write!(f, "I/O error: {}", e),
            SyncError::ConnectionFailed(msg) => write!(f, "{}", msg),
            SyncError::PushFailed(msg) => write!(f, "{}", msg),
            SyncError::PullFailed(msg) => write!(f, "{}", msg),
            SyncError::DeletionFailed(msg) => write!(f, "{}", msg),
            SyncError::StartupBlocked(msg) => write!(f, "{}", msg),
            SyncError::ConfigError(msg) => write!(f, "{}", msg),
            SyncError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for SyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SyncError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for SyncError {
    fn from(e: io::Error) -> Self {
        SyncError::IoError(e)
    }
}

pub fn apply_rsync_filters(cmd: &mut Command) {
    cmd.arg("--include=sessions/***")
        .arg("--exclude=storages/session_projcache.json")
        .arg("--include=storages/***")
        .arg("--include=attachments/")
        .arg("--include=attachments/v1/")
        .arg("--include=attachments/v1/objects/")
        .arg("--include=attachments/v1/objects/***")
        .arg("--exclude=attachments/v1/request-images/***")
        .arg("--exclude=attachments/***")
        .arg("--exclude=.sync-dirty")
        .arg("--exclude=credentials/***")
        .arg("--exclude=settings/***")
        .arg("--exclude=plugins/***")
        .arg("--exclude=presets/***")
        .arg("--exclude=profiles/***")
        .arg("--exclude=*");
}

#[async_trait::async_trait]
pub trait SyncRunner: Send + Sync {
    async fn verify_connection(&self, config: &SyncConfig) -> Result<(), SyncError>;
    async fn run_rsync_push(
        &self,
        config: &SyncConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), SyncError>;
    async fn run_rsync_pull(
        &self,
        config: &SyncConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), SyncError>;
    async fn delete_remote_store(
        &self,
        config: &SyncConfig,
        project_id: Uuid,
    ) -> Result<(), SyncError>;
    async fn is_local_state_dirty(&self, project_path: &Path) -> Result<bool, SyncError>;
    async fn mark_local_state_dirty(&self, project_path: &Path) -> Result<(), SyncError>;
    async fn mark_local_state_clean(&self, project_path: &Path) -> Result<(), SyncError>;
    async fn check_local_portable_state_exists(
        &self,
        project_path: &Path,
    ) -> Result<bool, SyncError>;
    async fn get_in_vm_sync_status(
        &self,
        project_path: &Path,
    ) -> Result<Option<SyncStatus>, SyncError>;
    async fn set_in_vm_sync_status(
        &self,
        project_path: &Path,
        status: SyncStatus,
        is_dirty: bool,
    ) -> Result<(), SyncError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDirection {
    Push,
    Pull,
}

#[derive(Clone, Debug)]
pub struct SystemSyncRunner {
    pub devvm_bin: PathBuf,
}

impl Default for SystemSyncRunner {
    fn default() -> Self {
        Self {
            devvm_bin: std::env::var("DEVVM_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("devvm")),
        }
    }
}

impl SystemSyncRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_devvm_bin(devvm_bin: PathBuf) -> Self {
        Self { devvm_bin }
    }

    fn ssh_bin(&self) -> PathBuf {
        self.devvm_bin
            .parent()
            .map(|p| p.join("ssh"))
            .filter(|p| p.is_file())
            .unwrap_or_else(|| PathBuf::from("ssh"))
    }

    pub fn build_rsync_command(
        &self,
        config: &SyncConfig,
        project_id: Uuid,
        project_path: &Path,
        direction: SyncDirection,
    ) -> Command {
        let port_str = config.ssh_port.to_string();
        let ssh_cmd = format!(
            "ssh -p {} -i {} -o StrictHostKeyChecking=accept-new -o BatchMode=yes",
            port_str,
            config.ssh_key_path.display()
        );
        let remote_dest = format!(
            "{}@{}:{}/{}/",
            config.ssh_user,
            config.ssh_host,
            config.remote_sync_root.trim_end_matches('/'),
            project_id
        );
        let vm_target_path = "/root/.dsh/";

        let (src, dst) = match direction {
            SyncDirection::Push => (vm_target_path, remote_dest.as_str()),
            SyncDirection::Pull => (remote_dest.as_str(), vm_target_path),
        };

        let mut cmd = Command::new(&self.devvm_bin);
        cmd.arg("exec")
            .arg("rsync")
            .arg("-avz")
            .arg("-e")
            .arg(&ssh_cmd);
        apply_rsync_filters(&mut cmd);
        cmd.arg(src).arg(dst).current_dir(project_path);
        cmd
    }

    async fn run_sync_direction(
        &self,
        config: &SyncConfig,
        project_id: Uuid,
        project_path: &Path,
        direction: SyncDirection,
    ) -> Result<(), SyncError> {
        let op_name = match direction {
            SyncDirection::Push => "push",
            SyncDirection::Pull => "pull",
        };

        if let SyncDirection::Push = direction {
            let port_str = config.ssh_port.to_string();
            let mkdir_cmd = format!(
                "mkdir -p \"{}/{}\"",
                config.remote_sync_root.trim_end_matches('/'),
                project_id
            );
            let host_key_path = resolve_host_ssh_key_path(&config.ssh_key_path);
            let mut ssh = Command::new(self.ssh_bin());
            ssh.arg("-p")
                .arg(&port_str)
                .arg("-i")
                .arg(&host_key_path)
                .arg("-o")
                .arg("StrictHostKeyChecking=accept-new")
                .arg("-o")
                .arg("BatchMode=yes")
                .arg(format!("{}@{}", config.ssh_user, config.ssh_host))
                .arg(&mkdir_cmd);
            let _ = ssh.output().await;
        }

        let mut cmd = self.build_rsync_command(config, project_id, project_path, direction);

        match cmd.output().await {
            Ok(output) => {
                if output.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let err_msg = format!(
                        "rsync {} failed (exit code {:?}): {}",
                        op_name,
                        output.status.code(),
                        stderr.trim()
                    );
                    match direction {
                        SyncDirection::Push => Err(SyncError::PushFailed(err_msg)),
                        SyncDirection::Pull => Err(SyncError::PullFailed(err_msg)),
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to execute rsync {}: {}", op_name, e);
                match direction {
                    SyncDirection::Push => Err(SyncError::PushFailed(err_msg)),
                    SyncDirection::Pull => Err(SyncError::PullFailed(err_msg)),
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl SyncRunner for SystemSyncRunner {
    async fn verify_connection(&self, config: &SyncConfig) -> Result<(), SyncError> {
        let port_str = config.ssh_port.to_string();
        let target = format!("{}@{}", config.ssh_user, config.ssh_host);
        let check_cmd = format!(
            "mkdir -p \"{0}\" && test -w \"{0}\"",
            config.remote_sync_root
        );
        let host_key_path = resolve_host_ssh_key_path(&config.ssh_key_path);

        let mut cmd = Command::new(self.ssh_bin());
        cmd.arg("-p")
            .arg(&port_str)
            .arg("-i")
            .arg(&host_key_path)
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(&target)
            .arg(&check_cmd);

        match cmd.output().await {
            Ok(output) => {
                if output.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(SyncError::ConnectionFailed(format!(
                        "SSH connection check failed (exit code {:?}): {}",
                        output.status.code(),
                        stderr.trim()
                    )))
                }
            }
            Err(e) => Err(SyncError::ConnectionFailed(format!(
                "Failed to execute ssh: {}",
                e
            ))),
        }
    }

    async fn run_rsync_push(
        &self,
        config: &SyncConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), SyncError> {
        self.run_sync_direction(config, project_id, project_path, SyncDirection::Push)
            .await
    }

    async fn run_rsync_pull(
        &self,
        config: &SyncConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), SyncError> {
        self.run_sync_direction(config, project_id, project_path, SyncDirection::Pull)
            .await
    }

    async fn delete_remote_store(
        &self,
        config: &SyncConfig,
        project_id: Uuid,
    ) -> Result<(), SyncError> {
        let port_str = config.ssh_port.to_string();
        let target = format!("{}@{}", config.ssh_user, config.ssh_host);
        let rm_cmd = format!(
            "rm -rf \"{}/{}\"",
            config.remote_sync_root.trim_end_matches('/'),
            project_id
        );
        let host_key_path = resolve_host_ssh_key_path(&config.ssh_key_path);

        let mut cmd = Command::new(self.ssh_bin());
        cmd.arg("-p")
            .arg(&port_str)
            .arg("-i")
            .arg(&host_key_path)
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(&target)
            .arg(&rm_cmd);

        match cmd.output().await {
            Ok(output) => {
                if output.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(SyncError::DeletionFailed(format!(
                        "Remote sync store deletion failed (exit code {:?}): {}",
                        output.status.code(),
                        stderr.trim()
                    )))
                }
            }
            Err(e) => Err(SyncError::DeletionFailed(format!(
                "Failed to execute ssh for sync deletion: {}",
                e
            ))),
        }
    }

    async fn is_local_state_dirty(&self, project_path: &Path) -> Result<bool, SyncError> {
        let mut cmd = Command::new(&self.devvm_bin);
        cmd.arg("exec")
            .arg("/bin/sh")
            .arg("-c")
            .arg("test -f /root/.dsh/.sync-dirty")
            .current_dir(project_path);

        match cmd.output().await {
            Ok(output) => Ok(output.status.success()),
            Err(e) => Err(SyncError::IoError(e)),
        }
    }

    async fn mark_local_state_dirty(&self, project_path: &Path) -> Result<(), SyncError> {
        let mut cmd = Command::new(&self.devvm_bin);
        cmd.arg("exec")
            .arg("/bin/sh")
            .arg("-c")
            .arg("mkdir -p /root/.dsh && touch /root/.dsh/.sync-dirty")
            .current_dir(project_path);

        match cmd.output().await {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(SyncError::Other(format!(
                "Failed to mark local state dirty in DevVM (exit code {:?})",
                output.status.code()
            ))),
            Err(e) => Err(SyncError::IoError(e)),
        }
    }

    async fn mark_local_state_clean(&self, project_path: &Path) -> Result<(), SyncError> {
        let mut cmd = Command::new(&self.devvm_bin);
        cmd.arg("exec")
            .arg("/bin/sh")
            .arg("-c")
            .arg("rm -f /root/.dsh/.sync-dirty")
            .current_dir(project_path);

        match cmd.output().await {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(SyncError::Other(format!(
                "Failed to mark local state clean in DevVM (exit code {:?})",
                output.status.code()
            ))),
            Err(e) => Err(SyncError::IoError(e)),
        }
    }

    async fn check_local_portable_state_exists(
        &self,
        project_path: &Path,
    ) -> Result<bool, SyncError> {
        let mut cmd = Command::new(&self.devvm_bin);
        cmd.arg("exec")
            .arg("/bin/sh")
            .arg("-c")
            .arg(
                "test -f /root/.dsh/storages/workspace.json || \
                 test -f /root/.dsh/storages/message_feedback.json || \
                 (test -d /root/.dsh/sessions && [ -n \"$(ls -A /root/.dsh/sessions 2>/dev/null)\" ]) || \
                 (test -d /root/.dsh/attachments/v1/objects && \
                  [ -n \"$(find /root/.dsh/attachments/v1/objects -type f -print -quit 2>/dev/null)\" ])"
            )
            .current_dir(project_path);

        match cmd.output().await {
            Ok(output) => Ok(output.status.success()),
            Err(e) => Err(SyncError::IoError(e)),
        }
    }

    async fn get_in_vm_sync_status(
        &self,
        project_path: &Path,
    ) -> Result<Option<SyncStatus>, SyncError> {
        let mut cmd = Command::new(&self.devvm_bin);
        cmd.arg("exec")
            .arg("/bin/sh")
            .arg("-c")
            .arg("cat /root/.dsh/.sync-status.json 2>/dev/null")
            .current_dir(project_path);

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    Ok(None)
                } else if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    if let Some(s) = val.get("status").and_then(|s| s.as_str()) {
                        let status = match s {
                            "synchronizing" => SyncStatus::Synchronizing,
                            "synchronized" => SyncStatus::Synchronized,
                            "degraded" => SyncStatus::Degraded,
                            "failed" => SyncStatus::Failed,
                            _ => SyncStatus::NotConfigured,
                        };
                        Ok(Some(status))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    async fn set_in_vm_sync_status(
        &self,
        project_path: &Path,
        status: SyncStatus,
        is_dirty: bool,
    ) -> Result<(), SyncError> {
        let status_str = match status {
            SyncStatus::NotConfigured => "not_configured",
            SyncStatus::Synchronizing => "synchronizing",
            SyncStatus::Synchronized => "synchronized",
            SyncStatus::Degraded => "degraded",
            SyncStatus::Failed => "failed",
        };
        let status_json = serde_json::json!({
            "status": status_str,
            "is_dirty": is_dirty,
        });
        let script = format!(
            "mkdir -p /root/.dsh && printf '%s\\n' '{}' > /root/.dsh/.sync-status.json",
            status_json
        );

        let mut cmd = Command::new(&self.devvm_bin);
        cmd.arg("exec")
            .arg("/bin/sh")
            .arg("-c")
            .arg(&script)
            .current_dir(project_path);

        let _ = cmd.output().await;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ProjectSyncState {
    status: Option<SyncStatus>,
    is_syncing: bool,
    pending_follow_up: bool,
    retry_count: u32,
}

#[derive(Clone)]
pub struct SyncManager {
    runner: Arc<dyn SyncRunner>,
    states: Arc<Mutex<HashMap<Uuid, ProjectSyncState>>>,
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            runner: Arc::new(SystemSyncRunner::new()),
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_devvm_bin(devvm_bin: PathBuf) -> Self {
        Self {
            runner: Arc::new(SystemSyncRunner::with_devvm_bin(devvm_bin)),
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_runner(runner: Arc<dyn SyncRunner>) -> Self {
        Self {
            runner,
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_status(&self, project_id: Uuid, is_configured: bool) -> SyncStatus {
        if !is_configured {
            return SyncStatus::NotConfigured;
        }
        let states = self.states.lock().await;
        states
            .get(&project_id)
            .and_then(|s| s.status)
            .unwrap_or(SyncStatus::NotConfigured)
    }

    pub async fn set_status(&self, project_id: Uuid, status: SyncStatus) {
        let mut states = self.states.lock().await;
        let entry = states.entry(project_id).or_default();
        entry.status = Some(status);
    }

    pub async fn verify_connection(&self, config: &SyncConfig) -> Result<(), SyncError> {
        self.runner.verify_connection(config).await
    }

    pub async fn reconcile_startup(
        &self,
        daemon_config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<SyncStatus, SyncError> {
        let sync_config_opt = load_sync_config(&daemon_config.sync_config_path)
            .map_err(|e| SyncError::ConfigError(format!("Failed to read sync config: {}", e)))?;

        let sync_config = match sync_config_opt {
            Some(c) => c,
            None => {
                self.set_status(project_id, SyncStatus::NotConfigured).await;
                return Ok(SyncStatus::NotConfigured);
            }
        };

        // Inspect state inside DevVM through runner (devvm exec)
        let is_dirty = self
            .runner
            .is_local_state_dirty(project_path)
            .await
            .unwrap_or(false);
        let local_exists = self
            .runner
            .check_local_portable_state_exists(project_path)
            .await
            .unwrap_or(false);

        let _ = append_log(
            &daemon_config.log_dir,
            project_id,
            "sync",
            "Checking Sync Store connectivity for startup reconciliation...",
        );

        match self.runner.verify_connection(&sync_config).await {
            Ok(()) => {
                if is_dirty {
                    let _ = append_log(
                        &daemon_config.log_dir,
                        project_id,
                        "sync",
                        "Dirty Local State detected. Pushing to Sync Store before launch...",
                    );
                    self.set_status(project_id, SyncStatus::Synchronizing).await;
                    let _ = self
                        .runner
                        .set_in_vm_sync_status(project_path, SyncStatus::Synchronizing, true)
                        .await;

                    if let Err(e) = self
                        .runner
                        .run_rsync_push(&sync_config, project_id, project_path)
                        .await
                    {
                        let msg = format!("Failed to push dirty local state to Sync Store: {}", e);
                        let _ = append_log(&daemon_config.log_dir, project_id, "sync:error", &msg);
                        self.set_status(project_id, SyncStatus::Failed).await;
                        let _ = self
                            .runner
                            .set_in_vm_sync_status(project_path, SyncStatus::Failed, true)
                            .await;
                        return Err(SyncError::PushFailed(msg));
                    }
                    let _ = self.runner.mark_local_state_clean(project_path).await;
                    self.set_status(project_id, SyncStatus::Synchronized).await;
                    let _ = self
                        .runner
                        .set_in_vm_sync_status(project_path, SyncStatus::Synchronized, false)
                        .await;
                    let _ = append_log(
                        &daemon_config.log_dir,
                        project_id,
                        "sync",
                        "Dirty Local State pushed to Sync Store successfully.",
                    );
                    Ok(SyncStatus::Synchronized)
                } else {
                    let _ = append_log(
                        &daemon_config.log_dir,
                        project_id,
                        "sync",
                        "Local state clean or fresh. Pulling Portable DSH State from Sync Store...",
                    );
                    self.set_status(project_id, SyncStatus::Synchronizing).await;
                    let _ = self
                        .runner
                        .set_in_vm_sync_status(project_path, SyncStatus::Synchronizing, false)
                        .await;

                    if let Err(e) = self
                        .runner
                        .run_rsync_pull(&sync_config, project_id, project_path)
                        .await
                    {
                        let msg = format!("Failed to pull state from Sync Store: {}", e);
                        let _ = append_log(&daemon_config.log_dir, project_id, "sync:error", &msg);
                        self.set_status(project_id, SyncStatus::Failed).await;
                        let _ = self
                            .runner
                            .set_in_vm_sync_status(project_path, SyncStatus::Failed, false)
                            .await;
                        return Err(SyncError::PullFailed(msg));
                    }
                    self.set_status(project_id, SyncStatus::Synchronized).await;
                    let _ = self
                        .runner
                        .set_in_vm_sync_status(project_path, SyncStatus::Synchronized, false)
                        .await;
                    let _ = append_log(
                        &daemon_config.log_dir,
                        project_id,
                        "sync",
                        "Portable DSH State pulled from Sync Store successfully.",
                    );
                    Ok(SyncStatus::Synchronized)
                }
            }
            Err(conn_err) => {
                if local_exists {
                    let warn_msg = format!(
                        "Sync Store is unreachable ({}); proceeding with startup in Degraded Sync status.",
                        conn_err
                    );
                    let _ = append_log(&daemon_config.log_dir, project_id, "sync:warn", &warn_msg);
                    self.set_status(project_id, SyncStatus::Degraded).await;
                    let _ = self
                        .runner
                        .set_in_vm_sync_status(project_path, SyncStatus::Degraded, is_dirty)
                        .await;
                    Ok(SyncStatus::Degraded)
                } else {
                    let block_msg = format!(
                        "Cannot launch DSH: Sync is configured but Sync Store is unreachable ({}) and no local Portable DSH State exists (preventing divergent empty history).",
                        conn_err
                    );
                    let _ =
                        append_log(&daemon_config.log_dir, project_id, "sync:error", &block_msg);
                    self.set_status(project_id, SyncStatus::Failed).await;
                    let _ = self
                        .runner
                        .set_in_vm_sync_status(project_path, SyncStatus::Failed, false)
                        .await;
                    Err(SyncError::StartupBlocked(block_msg))
                }
            }
        }
    }

    pub async fn trigger_sync(
        &self,
        daemon_config: &DaemonConfig,
        project_id: Uuid,
        project_path: &Path,
    ) -> Result<SyncStatus, SyncError> {
        let sync_config_opt = load_sync_config(&daemon_config.sync_config_path)
            .map_err(|e| SyncError::ConfigError(format!("Failed to read sync config: {}", e)))?;

        let sync_config = match sync_config_opt {
            Some(c) => c,
            None => {
                return Err(SyncError::NotConfigured);
            }
        };

        // Mark local state dirty inside DevVM before background sync begins
        let _ = self.runner.mark_local_state_dirty(project_path).await;
        let _ = self
            .runner
            .set_in_vm_sync_status(project_path, SyncStatus::Synchronizing, true)
            .await;

        // Check if a transfer is already running for this project
        {
            let mut states = self.states.lock().await;
            let state = states.entry(project_id).or_default();
            if state.is_syncing {
                state.pending_follow_up = true;
                return Ok(SyncStatus::Synchronizing);
            }
            state.is_syncing = true;
            state.pending_follow_up = false;
            state.status = Some(SyncStatus::Synchronizing);
        }

        let _ = append_log(
            &daemon_config.log_dir,
            project_id,
            "sync",
            "Starting synchronization...",
        );

        let runner = Arc::clone(&self.runner);
        let states_arc = Arc::clone(&self.states);
        let log_dir = daemon_config.log_dir.clone();
        let project_path_buf = project_path.to_path_buf();

        tokio::spawn(async move {
            let mut keep_running = true;

            while keep_running {
                let mut retry_count = 0;
                let mut success = false;
                let mut last_error = String::new();

                while retry_count < 5 {
                    match runner
                        .run_rsync_push(&sync_config, project_id, &project_path_buf)
                        .await
                    {
                        Ok(()) => {
                            success = true;
                            break;
                        }
                        Err(e) => {
                            retry_count += 1;
                            last_error = e.to_string();
                            let _ = append_log(
                                &log_dir,
                                project_id,
                                "sync:warn",
                                &format!(
                                    "Transfer attempt {} failed: {}. Retrying in 1s...",
                                    retry_count, last_error
                                ),
                            );
                            if retry_count < 5 {
                                tokio::time::sleep(Duration::from_millis(1000)).await;
                            }
                        }
                    }
                }

                let mut states = states_arc.lock().await;
                let state = states.entry(project_id).or_default();

                if success {
                    let _ = runner.mark_local_state_clean(&project_path_buf).await;
                    let _ = runner
                        .set_in_vm_sync_status(&project_path_buf, SyncStatus::Synchronized, false)
                        .await;
                    state.status = Some(SyncStatus::Synchronized);
                    state.retry_count = 0;
                    let _ = append_log(
                        &log_dir,
                        project_id,
                        "sync",
                        "Synchronization succeeded. State marked clean.",
                    );

                    if state.pending_follow_up {
                        state.pending_follow_up = false;
                        keep_running = true;
                    } else {
                        state.is_syncing = false;
                        keep_running = false;
                    }
                } else {
                    let _ = runner
                        .set_in_vm_sync_status(&project_path_buf, SyncStatus::Failed, true)
                        .await;
                    state.status = Some(SyncStatus::Failed);
                    state.retry_count = retry_count;
                    state.is_syncing = false;
                    keep_running = false;
                    let _ = append_log(
                        &log_dir,
                        project_id,
                        "sync:error",
                        &format!(
                            "Synchronization failed after 5 attempts: {}. Retaining dirty local state.",
                            last_error
                        ),
                    );
                }
            }
        });

        Ok(SyncStatus::Synchronizing)
    }

    pub async fn delete_sync_store(
        &self,
        daemon_config: &DaemonConfig,
        project_id: Uuid,
        confirmed: bool,
    ) -> Result<(), SyncError> {
        if !confirmed {
            return Err(SyncError::ConfirmationRequired);
        }

        let sync_config_opt = load_sync_config(&daemon_config.sync_config_path)
            .map_err(|e| SyncError::ConfigError(format!("Failed to read sync config: {}", e)))?;

        let sync_config = match sync_config_opt {
            Some(c) => c,
            None => {
                return Err(SyncError::NotConfigured);
            }
        };

        let _ = append_log(
            &daemon_config.log_dir,
            project_id,
            "sync",
            &format!(
                "Deleting remote Sync Store data at {}/{}...",
                sync_config.remote_sync_root, project_id
            ),
        );

        match self
            .runner
            .delete_remote_store(&sync_config, project_id)
            .await
        {
            Ok(()) => {
                self.set_status(project_id, SyncStatus::NotConfigured).await;
                let _ = append_log(
                    &daemon_config.log_dir,
                    project_id,
                    "sync",
                    "Remote Sync Store data deleted successfully.",
                );
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("Failed to delete remote Sync Store data: {}", e);
                let _ = append_log(&daemon_config.log_dir, project_id, "sync:error", &err_msg);
                Err(SyncError::DeletionFailed(err_msg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_sync_config_serde_and_persistence() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sync.json");

        // Initially absent
        assert!(load_sync_config(&config_path).unwrap().is_none());

        let cfg = SyncConfig {
            ssh_user: "devvm".to_string(),
            ssh_host: "10.0.0.1".to_string(),
            ssh_port: 2222,
            ssh_key_path: PathBuf::from("/home/user/.ssh/id_ed25519"),
            remote_sync_root: "/var/lib/devvm-sync".to_string(),
        };

        save_sync_config(&config_path, &cfg).unwrap();
        assert!(config_path.exists());

        let loaded = load_sync_config(&config_path).unwrap().unwrap();
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn test_dirty_flag_lifecycle() {
        let dir = tempdir().unwrap();
        let dsh_path = dir.path().join(".dsh");

        assert!(!is_local_state_dirty(&dsh_path));
        mark_local_state_dirty(&dsh_path).unwrap();
        assert!(is_local_state_dirty(&dsh_path));
        mark_local_state_clean(&dsh_path).unwrap();
        assert!(!is_local_state_dirty(&dsh_path));
    }

    #[test]
    fn test_check_local_portable_state_exists() {
        let dir = tempdir().unwrap();
        let dsh_path = dir.path().join(".dsh");

        assert!(!check_local_portable_state_exists(&dsh_path));

        // Create sessions directory with one file
        let sessions_dir = dsh_path.join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();
        assert!(!check_local_portable_state_exists(&dsh_path)); // empty sessions dir
        fs::write(sessions_dir.join("session.jsonl"), "{}\n").unwrap();
        assert!(check_local_portable_state_exists(&dsh_path));

        // Workspace and feedback domains are authoritative; projection cache alone is not.
        let dir2 = tempdir().unwrap();
        let dsh_path2 = dir2.path().join(".dsh");
        fs::create_dir_all(dsh_path2.join("storages")).unwrap();
        fs::write(dsh_path2.join("storages/session_projcache.json"), "{}\n").unwrap();
        assert!(!check_local_portable_state_exists(&dsh_path2));
        fs::write(dsh_path2.join("storages/workspace.json"), "{}\n").unwrap();
        assert!(check_local_portable_state_exists(&dsh_path2));

        // An authoritative attachment object is Portable DSH State.
        let dir3 = tempdir().unwrap();
        let dsh_path3 = dir3.path().join(".dsh");
        let object_dir = dsh_path3.join("attachments/v1/objects/ab");
        fs::create_dir_all(&object_dir).unwrap();
        fs::write(object_dir.join("abcdef"), b"image").unwrap();
        assert!(check_local_portable_state_exists(&dsh_path3));
    }

    #[test]
    fn test_apply_rsync_filters() {
        let mut cmd = Command::new("rsync");
        apply_rsync_filters(&mut cmd);
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            args,
            [
                "--include=sessions/***",
                "--exclude=storages/session_projcache.json",
                "--include=storages/***",
                "--include=attachments/",
                "--include=attachments/v1/",
                "--include=attachments/v1/objects/",
                "--include=attachments/v1/objects/***",
                "--exclude=attachments/v1/request-images/***",
                "--exclude=attachments/***",
                "--exclude=.sync-dirty",
                "--exclude=credentials/***",
                "--exclude=settings/***",
                "--exclude=plugins/***",
                "--exclude=presets/***",
                "--exclude=profiles/***",
                "--exclude=*",
            ]
        );
    }

    #[test]
    fn test_sync_error_display_and_source() {
        assert_eq!(
            SyncError::NotConfigured.to_string(),
            "Sync is not configured"
        );
        assert_eq!(
            SyncError::ConfirmationRequired.to_string(),
            "Confirmation required: confirmed must be true"
        );
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let sync_err: SyncError = io_err.into();
        assert!(sync_err.to_string().contains("file not found"));
        use std::error::Error;
        assert!(sync_err.source().is_some());
    }

    #[test]
    fn test_build_rsync_command_push_and_pull() {
        let runner = SystemSyncRunner::with_devvm_bin(PathBuf::from("/custom/bin/devvm"));
        let config = SyncConfig {
            ssh_user: "devvm-user".to_string(),
            ssh_host: "sync.internal".to_string(),
            ssh_port: 2222,
            ssh_key_path: PathBuf::from("/root/.ssh/id_ed25519"),
            remote_sync_root: "/var/lib/devvm-sync".to_string(),
        };
        let project_id = Uuid::new_v4();
        let project_path = PathBuf::from("/home/user/my-project");

        // Test Push command
        let push_cmd =
            runner.build_rsync_command(&config, project_id, &project_path, SyncDirection::Push);
        let push_args: Vec<String> = push_cmd
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert_eq!(push_args[0], "exec");
        assert_eq!(push_args[1], "rsync");
        assert_eq!(push_args[2], "-avz");
        assert_eq!(push_args[3], "-e");
        assert!(push_args[4].contains("ssh -p 2222 -i /root/.ssh/id_ed25519"));
        // Push source is /root/.dsh/ and target is remote
        assert_eq!(push_args[push_args.len() - 2], "/root/.dsh/");
        assert_eq!(
            push_args[push_args.len() - 1],
            format!(
                "devvm-user@sync.internal:/var/lib/devvm-sync/{}/",
                project_id
            )
        );

        // Test Pull command
        let pull_cmd =
            runner.build_rsync_command(&config, project_id, &project_path, SyncDirection::Pull);
        let pull_args: Vec<String> = pull_cmd
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert_eq!(pull_args[0], "exec");
        assert_eq!(pull_args[1], "rsync");
        // Pull source is remote and target is /root/.dsh/
        assert_eq!(
            pull_args[pull_args.len() - 2],
            format!(
                "devvm-user@sync.internal:/var/lib/devvm-sync/{}/",
                project_id
            )
        );
        assert_eq!(pull_args[pull_args.len() - 1], "/root/.dsh/");
    }

    #[test]
    fn test_provision_sync_setup_host_and_guest_paths() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempdir().unwrap();
        let daemon_config_path = temp_dir.path().join("daemon-sync.json");
        let devvm_root = temp_dir.path().join("mock_devvm_root");
        fs::create_dir_all(&devvm_root).unwrap();

        // Create a host temp SSH key
        let host_ssh_key = temp_dir.path().join("host_id_ed25519");
        fs::write(&host_ssh_key, "mock-private-key-data\n").unwrap();

        std::env::set_var("DEVVM_ROOT", &devvm_root);

        let sync_config = SyncConfig {
            ssh_user: "devvm-user".to_string(),
            ssh_host: "sync.vps.net".to_string(),
            ssh_port: 2222,
            ssh_key_path: host_ssh_key.clone(),
            remote_sync_root: "/data/sync".to_string(),
        };

        let result = provision_sync_setup(&daemon_config_path, &sync_config).unwrap();
        assert_eq!(
            result.ssh_key_path,
            PathBuf::from("/root/.ssh/host_id_ed25519")
        );

        // Host daemon config must retain original host key path
        let host_saved = load_sync_config(&daemon_config_path).unwrap().unwrap();
        assert_eq!(host_saved.ssh_key_path, host_ssh_key);

        // Guest config must have rewritten guest key path
        let guest_config_path = devvm_root.join(".config/devvm/sync.json");
        let guest_saved = load_sync_config(&guest_config_path).unwrap().unwrap();
        assert_eq!(
            guest_saved.ssh_key_path,
            PathBuf::from("/root/.ssh/host_id_ed25519")
        );

        // Check directory permissions 0700
        let ssh_dir = devvm_root.join(".ssh");
        let dir_perms = fs::metadata(&ssh_dir).unwrap().permissions();
        assert_eq!(dir_perms.mode() & 0o777, 0o700);

        // Check file permissions 0600
        let copied_key = ssh_dir.join("host_id_ed25519");
        assert!(copied_key.is_file());
        let file_perms = fs::metadata(&copied_key).unwrap().permissions();
        assert_eq!(file_perms.mode() & 0o777, 0o600);

        let copied_content = fs::read_to_string(&copied_key).unwrap();
        assert_eq!(copied_content, "mock-private-key-data\n");
    }
}
