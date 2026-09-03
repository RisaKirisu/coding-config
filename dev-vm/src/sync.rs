use crate::config::DaemonConfig;
use crate::logs::append_log_logged;
use crate::models::SyncStatus;
use crate::runner::{log_command_failure, log_command_spawn_failure};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_url: Option<String>,
}

fn default_ssh_port() -> u16 {
    22
}

fn default_remote_sync_root() -> String {
    "/var/lib/devvm-sync".to_string()
}

/// Quotes a value for safe interpolation into a single remote `/bin/sh` command string.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
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

pub(crate) fn devvm_root() -> PathBuf {
    std::env::var("DEVVM_ROOT")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("DEVVM_HOME").map(|h| PathBuf::from(h).join("root")))
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join("coding-config/dev-vm/root"))
                .unwrap_or_else(|| PathBuf::from("/root/.local/share/devvm/root"))
        })
}

pub fn resolve_host_ssh_key_path(guest_or_host_path: &Path) -> PathBuf {
    if guest_or_host_path.is_file() {
        return guest_or_host_path.to_path_buf();
    }
    let path_str = guest_or_host_path.to_string_lossy();
    if let Some(subpath) = path_str.strip_prefix("/root/") {
        let in_devvm_root = devvm_root().join(subpath);
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
    daemon_url: &str,
) -> Result<SyncConfig, io::Error> {
    save_sync_config(daemon_config_path, sync_config)?;

    let devvm_root = devvm_root();
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

    let shared_config_path = shared_config_dir.join("sync.json");

    // A workstation keeps one writer_id for the life of its Sync Store participation,
    // so re-provisioning must never mint a new one (ADR 0005 tiebreak identity).
    let existing_writer_id = load_sync_config(&shared_config_path)
        .ok()
        .flatten()
        .and_then(|c| c.writer_id);

    let mut guest_config = sync_config.clone();
    guest_config.ssh_key_path = guest_key_path;
    guest_config.writer_id = Some(existing_writer_id.unwrap_or_else(|| Uuid::new_v4().to_string()));
    guest_config.daemon_url = Some(daemon_url.to_string());

    save_sync_config(&shared_config_path, &guest_config)?;

    Ok(guest_config)
}

#[derive(Debug)]
pub enum SyncError {
    NotConfigured,
    ConfirmationRequired,
    Failed(String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::NotConfigured => write!(f, "Sync is not configured"),
            SyncError::ConfirmationRequired => {
                write!(f, "Confirmation required: confirmed must be true")
            }
            SyncError::Failed(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<io::Error> for SyncError {
    fn from(e: io::Error) -> Self {
        SyncError::Failed(e.to_string())
    }
}

#[async_trait::async_trait]
pub trait SyncRunner: Send + Sync {
    /// Host-side reachability check, used only while provisioning Sync Store credentials.
    async fn verify(&self, config: &SyncConfig) -> Result<(), SyncError>;
    async fn delete_store(&self, config: &SyncConfig, project_id: Uuid) -> Result<(), SyncError>;
    async fn read_status(&self, project_path: &Path) -> Result<Option<SyncStatus>, SyncError>;
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

    pub fn verify_remote_command(config: &SyncConfig) -> String {
        let root = shell_quote(&config.remote_sync_root);
        format!("mkdir -p {0} && test -w {0}", root)
    }

    pub fn delete_remote_command(config: &SyncConfig, project_id: Uuid) -> String {
        format!(
            "rm -rf {}",
            shell_quote(&format!(
                "{}/{}",
                config.remote_sync_root.trim_end_matches('/'),
                project_id
            ))
        )
    }

    fn ssh_args(config: &SyncConfig, remote_command: &str) -> Vec<String> {
        vec![
            "-p".to_string(),
            config.ssh_port.to_string(),
            "-i".to_string(),
            resolve_host_ssh_key_path(&config.ssh_key_path)
                .display()
                .to_string(),
            "-o".to_string(),
            "StrictHostKeyChecking=accept-new".to_string(),
            "-o".to_string(),
            "ConnectTimeout=5".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            format!("{}@{}", config.ssh_user, config.ssh_host),
            remote_command.to_string(),
        ]
    }

    fn ssh_command(&self, config: &SyncConfig, remote_command: &str) -> Command {
        let mut cmd = Command::new(self.ssh_bin());
        cmd.args(Self::ssh_args(config, remote_command));
        cmd
    }
}

#[async_trait::async_trait]
impl SyncRunner for SystemSyncRunner {
    async fn verify(&self, config: &SyncConfig) -> Result<(), SyncError> {
        let remote_command = Self::verify_remote_command(config);
        let program = self.ssh_bin().display().to_string();
        let args = Self::ssh_args(config, &remote_command);
        let mut cmd = self.ssh_command(config, &remote_command);
        match cmd.output().await {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                log_command_failure(&program, &args, &output);
                Err(SyncError::Failed(format!(
                    "SSH connection check failed (exit code {:?}): {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                )))
            }
            Err(e) => {
                log_command_spawn_failure(&program, &args, &e);
                Err(SyncError::Failed(format!("Failed to execute ssh: {}", e)))
            }
        }
    }

    async fn delete_store(&self, config: &SyncConfig, project_id: Uuid) -> Result<(), SyncError> {
        let remote_command = Self::delete_remote_command(config, project_id);
        let program = self.ssh_bin().display().to_string();
        let args = Self::ssh_args(config, &remote_command);
        let mut cmd = self.ssh_command(config, &remote_command);
        match cmd.output().await {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                log_command_failure(&program, &args, &output);
                Err(SyncError::Failed(format!(
                    "Remote sync store deletion failed (exit code {:?}): {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                )))
            }
            Err(e) => {
                log_command_spawn_failure(&program, &args, &e);
                Err(SyncError::Failed(format!(
                    "Failed to execute ssh for sync deletion: {}",
                    e
                )))
            }
        }
    }

    async fn read_status(&self, project_path: &Path) -> Result<Option<SyncStatus>, SyncError> {
        let status_command = "cat /run/devvm/sync-status.json 2>/dev/null";
        let program = self.devvm_bin.display().to_string();
        let args = [
            "exec".to_string(),
            "/bin/sh".to_string(),
            "-c".to_string(),
            status_command.to_string(),
        ];
        let mut cmd = Command::new(&self.devvm_bin);
        cmd.arg("exec")
            .arg("/bin/sh")
            .arg("-c")
            .arg(status_command)
            .current_dir(project_path);

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                Ok(parse_status_file(&String::from_utf8_lossy(&output.stdout)))
            }
            // A non-zero exit means the Sync Status file does not exist yet, which is the
            // ordinary state, so it is reported at debug level rather than as an error.
            Ok(output) => {
                tracing::debug!(
                    program,
                    args = ?args,
                    exit_code = ?output.status.code(),
                    stderr = %String::from_utf8_lossy(&output.stderr),
                    "no Sync Status file inside DevVM"
                );
                Ok(None)
            }
            Err(e) => {
                log_command_spawn_failure(&program, &args, &e);
                Err(SyncError::Failed(format!(
                    "Failed to read Sync Status inside DevVM: {}",
                    e
                )))
            }
        }
    }
}

#[derive(Deserialize)]
struct SyncStatusFile {
    status: SyncStatus,
}

fn parse_status_file(contents: &str) -> Option<SyncStatus> {
    serde_json::from_str::<SyncStatusFile>(contents)
        .ok()
        .map(|f| f.status)
}

#[derive(Clone)]
pub struct SyncManager {
    runner: Arc<dyn SyncRunner>,
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
        }
    }

    pub fn with_devvm_bin(devvm_bin: PathBuf) -> Self {
        Self {
            runner: Arc::new(SystemSyncRunner::with_devvm_bin(devvm_bin)),
        }
    }

    pub fn with_runner(runner: Arc<dyn SyncRunner>) -> Self {
        Self { runner }
    }

    pub async fn verify(&self, config: &SyncConfig) -> Result<(), SyncError> {
        self.runner.verify(config).await
    }

    /// An unreadable status file is indistinguishable from an unreported one: the UI shows no badge.
    pub async fn read_status(&self, project_path: &Path) -> Option<SyncStatus> {
        self.runner.read_status(project_path).await.ok().flatten()
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

        let sync_config = load_sync_config(&daemon_config.sync_config_path)
            .map_err(|e| SyncError::Failed(format!("Failed to read sync config: {}", e)))?
            .ok_or(SyncError::NotConfigured)?;

        append_log_logged(
            &daemon_config.log_dir,
            project_id,
            "sync",
            &format!(
                "Deleting remote Sync Store data at {}/{}...",
                sync_config.remote_sync_root, project_id
            ),
        );

        match self.runner.delete_store(&sync_config, project_id).await {
            Ok(()) => {
                append_log_logged(
                    &daemon_config.log_dir,
                    project_id,
                    "sync",
                    "Remote Sync Store data deleted successfully.",
                );
                Ok(())
            }
            Err(e) => Err(SyncError::Failed(format!(
                "Failed to delete remote Sync Store data: {}",
                e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_config() -> SyncConfig {
        SyncConfig {
            ssh_user: "devvm".to_string(),
            ssh_host: "10.0.0.1".to_string(),
            ssh_port: 2222,
            ssh_key_path: PathBuf::from("/home/user/.ssh/id_ed25519"),
            remote_sync_root: "/var/lib/devvm-sync".to_string(),
            writer_id: None,
            daemon_url: None,
        }
    }

    #[test]
    fn test_sync_config_serde_and_persistence() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sync.json");

        assert!(load_sync_config(&config_path).unwrap().is_none());

        let mut cfg = test_config();
        cfg.writer_id = Some(Uuid::new_v4().to_string());
        cfg.daemon_url = Some("http://127.0.0.1:8100".to_string());

        save_sync_config(&config_path, &cfg).unwrap();
        let loaded = load_sync_config(&config_path).unwrap().unwrap();
        assert_eq!(loaded, cfg);

        // Optional fields are omitted when absent and tolerated when missing on load.
        let bare = test_config();
        save_sync_config(&config_path, &bare).unwrap();
        let raw = fs::read_to_string(&config_path).unwrap();
        assert!(!raw.contains("writer_id"));
        assert!(!raw.contains("daemon_url"));
        assert_eq!(load_sync_config(&config_path).unwrap().unwrap(), bare);
    }

    #[test]
    fn test_shell_quote() {
        assert_eq!(shell_quote("/var/lib/devvm-sync"), "'/var/lib/devvm-sync'");
        assert_eq!(shell_quote("/tmp/x; rm -rf /"), "'/tmp/x; rm -rf /'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn test_remote_commands_quote_the_sync_root() {
        let mut config = test_config();
        config.remote_sync_root = "/tmp/x; rm -rf /".to_string();
        let project_id = Uuid::new_v4();

        let verify_cmd = SystemSyncRunner::verify_remote_command(&config);
        assert_eq!(
            verify_cmd,
            "mkdir -p '/tmp/x; rm -rf /' && test -w '/tmp/x; rm -rf /'"
        );

        let delete_cmd = SystemSyncRunner::delete_remote_command(&config, project_id);
        assert_eq!(
            delete_cmd,
            format!("rm -rf '/tmp/x; rm -rf /{}'", project_id)
        );
    }

    #[test]
    fn test_parse_status_file() {
        for (raw, expected) in [
            ("not_configured", SyncStatus::NotConfigured),
            ("synchronizing", SyncStatus::Synchronizing),
            ("synchronized", SyncStatus::Synchronized),
            ("remote_ahead", SyncStatus::RemoteAhead),
            ("degraded", SyncStatus::Degraded),
            ("failed", SyncStatus::Failed),
        ] {
            let json = format!(
                r#"{{"status":"{}","head_seq":3,"last_error":null,"updated_at":"2024-01-01T00:00:00Z"}}"#,
                raw
            );
            assert_eq!(parse_status_file(&json), Some(expected));
        }

        assert_eq!(parse_status_file(""), None);
        assert_eq!(parse_status_file("not json at all"), None);
        assert_eq!(parse_status_file(r#"{"status":"bogus"}"#), None);
        assert_eq!(parse_status_file(r#"{"head_seq":1}"#), None);
    }

    #[test]
    fn test_sync_error_display() {
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
    }

    #[test]
    fn test_provision_sync_setup_host_and_guest_paths() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempdir().unwrap();
        let daemon_config_path = temp_dir.path().join("daemon-sync.json");
        let devvm_root = temp_dir.path().join("mock_devvm_root");
        fs::create_dir_all(&devvm_root).unwrap();

        let host_ssh_key = temp_dir.path().join("host_id_ed25519");
        fs::write(&host_ssh_key, "mock-private-key-data\n").unwrap();

        std::env::set_var("DEVVM_ROOT", &devvm_root);

        let sync_config = SyncConfig {
            ssh_user: "devvm-user".to_string(),
            ssh_host: "sync.vps.net".to_string(),
            ssh_port: 2222,
            ssh_key_path: host_ssh_key.clone(),
            remote_sync_root: "/data/sync".to_string(),
            writer_id: None,
            daemon_url: None,
        };

        let result =
            provision_sync_setup(&daemon_config_path, &sync_config, "http://127.0.0.1:8100")
                .unwrap();
        assert_eq!(
            result.ssh_key_path,
            PathBuf::from("/root/.ssh/host_id_ed25519")
        );
        assert_eq!(result.daemon_url.as_deref(), Some("http://127.0.0.1:8100"));
        let writer_id = result.writer_id.clone().unwrap();
        Uuid::parse_str(&writer_id).expect("writer_id must be a UUID");

        // Host daemon config keeps the original host key path.
        let host_saved = load_sync_config(&daemon_config_path).unwrap().unwrap();
        assert_eq!(host_saved.ssh_key_path, host_ssh_key);

        let guest_config_path = devvm_root.join(".config/devvm/sync.json");
        let guest_saved = load_sync_config(&guest_config_path).unwrap().unwrap();
        assert_eq!(
            guest_saved.ssh_key_path,
            PathBuf::from("/root/.ssh/host_id_ed25519")
        );
        assert_eq!(guest_saved.writer_id, Some(writer_id.clone()));

        let ssh_dir = devvm_root.join(".ssh");
        assert_eq!(
            fs::metadata(&ssh_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let copied_key = ssh_dir.join("host_id_ed25519");
        assert_eq!(
            fs::metadata(&copied_key).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_to_string(&copied_key).unwrap(),
            "mock-private-key-data\n"
        );

        // Re-provisioning preserves the workstation writer_id.
        let second =
            provision_sync_setup(&daemon_config_path, &sync_config, "http://127.0.0.1:9100")
                .unwrap();
        assert_eq!(second.writer_id, Some(writer_id));
        assert_eq!(second.daemon_url.as_deref(), Some("http://127.0.0.1:9100"));
    }
}
