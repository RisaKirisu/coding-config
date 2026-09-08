use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: Uuid,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VmStatus {
    Running,
    Stopped,
    Failed,
    Unknown,
}

/// A crashed DSH Runtime reads back as `Stopped`; the cause is in the Project's `dsh.log`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DshStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    #[serde(alias = "not_synchronized")]
    NotConfigured,
    Synchronizing,
    Synchronized,
    RemoteAhead,
    Degraded,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectLinks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_dsh_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailnet_dsh_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dsh_url: Option<String>,
    pub local_port_template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailnet_port_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_url_template: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectView {
    pub id: Uuid,
    pub path: String,
    pub name: String,
    pub project_host: String,
    pub vm_status: VmStatus,
    pub dsh_status: DshStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_status: Option<SyncStatus>,
    pub links: ProjectLinks,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserResult {
    pub current: String,
    pub parent: Option<String>,
    pub entries: Vec<BrowserEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RegisterRequest {
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogsResponse {
    pub project_id: Uuid,
    pub entries: Vec<crate::logs::LogEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenPortRequest {
    pub port: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenPortResponse {
    pub local_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailnet_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncSetupRequest {
    pub ssh_user: String,
    pub ssh_host: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    pub ssh_key_path: PathBuf,
    #[serde(default = "default_remote_sync_root")]
    pub remote_sync_root: String,
    #[serde(default = "default_true")]
    pub verify: bool,
}

fn default_ssh_port() -> u16 {
    22
}

fn default_remote_sync_root() -> String {
    "/var/lib/devvm-sync".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncConfigResponse {
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<crate::sync::SyncConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncDeleteRequest {
    #[serde(default)]
    pub confirmed: bool,
}

pub fn compute_project_host(project_path: &Path) -> String {
    let dir_name = project_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");

    let mut sanitized = String::new();
    let mut last_dash = false;
    for c in dir_name.chars() {
        let lower = c.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() || lower == '-' {
            sanitized.push(lower);
            last_dash = false;
        } else if !last_dash {
            sanitized.push('-');
            last_dash = true;
        }
    }
    let trimmed = sanitized.trim_matches('-');
    let project_name = if trimmed.is_empty() {
        "project"
    } else {
        trimmed
    };

    let path_str = project_path.to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let hash_result = hasher.finalize();
    let hash_hex = format!("{:x}", hash_result);
    let project_hash = &hash_hex[..8];

    // Leave room for the dash and five-digit port in a 63-character DNS label.
    if project_name.len() > 48 {
        project_hash.to_string()
    } else {
        format!("{}-{}", project_name, project_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_project_host() {
        let path = Path::new("/root/dev-vm");
        let host = compute_project_host(path);
        assert!(host.starts_with("dev-vm-"));
        assert_eq!(host.len(), "dev-vm-".len() + 8);
    }

    #[test]
    fn test_long_project_name_uses_path_hash() {
        let long_name = "a".repeat(100);
        let path = PathBuf::from(format!("/root/{}", long_name));
        let host = compute_project_host(&path);
        assert_eq!(host.len(), 8);
        assert!(host.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(
            host,
            compute_project_host(&PathBuf::from(format!("/other/{}", long_name)))
        );
    }
}
