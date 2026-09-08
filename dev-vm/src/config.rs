use crate::service::default_home_dir;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct DaemonConfig {
    pub host: String,
    pub port: u16,
    pub config_path: PathBuf,
    pub sync_config_path: PathBuf,
    pub log_dir: PathBuf,
    pub home_dir: PathBuf,
    pub devvm_bin: PathBuf,
    pub ingress_port: u16,
    pub remote_domain: Option<String>,
}

pub fn determine_bind_address(
    host: Option<&str>,
    port: u16,
) -> Result<SocketAddr, std::net::AddrParseError> {
    let host = host.filter(|s| !s.is_empty()).unwrap_or("127.0.0.1");
    Ok(SocketAddr::new(host.parse()?, port))
}

impl DaemonConfig {
    pub fn new() -> Self {
        let home_dir = std::env::var("DEVVM_HOME_DIR")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(PathBuf::from))
            .unwrap_or_else(|_| default_home_dir());

        let config_dir_base = std::env::var("DEVVM_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::config_dir()
                    .map(|p| p.join("devvm"))
                    .unwrap_or_else(|| home_dir.join(".config/devvm"))
            });

        let config_path = std::env::var("DEVVM_CONFIG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| config_dir_base.join("projects.json"));

        let sync_config_path = std::env::var("DEVVM_SYNC_CONFIG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| config_dir_base.join("sync.json"));

        // Project logs live in the DevVM root so the guest sees them at
        // /devvm-root/.project-logs and writes dsh.log and ingress.log beside daemon.log.
        let log_dir = std::env::var("DEVVM_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| crate::sync::devvm_root().join(".project-logs"));

        let devvm_bin = std::env::var("DEVVM_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("devvm"));

        let port = std::env::var("DEVVM_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8100);

        let host = std::env::var("DEVVM_HOST").unwrap_or_default();

        let ingress_port = std::env::var("DEVVM_INGRESS_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8102);

        let remote_domain = std::env::var("DEVVM_REMOTE_DOMAIN")
            .ok()
            .filter(|s| !s.trim().is_empty());

        Self {
            host,
            port,
            config_path,
            sync_config_path,
            log_dir,
            home_dir,
            devvm_bin,
            ingress_port,
            remote_domain,
        }
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_address_defaults_to_loopback_and_rejects_invalid_input() {
        for host in [None, Some(""), Some("127.0.0.1")] {
            assert_eq!(
                determine_bind_address(host, 8100).unwrap(),
                SocketAddr::from(([127, 0, 0, 1], 8100))
            );
        }
        assert_eq!(
            determine_bind_address(Some("100.67.154.69"), 9000).unwrap(),
            SocketAddr::from(([100, 67, 154, 69], 9000))
        );
        assert!(determine_bind_address(Some("invalid"), 8100).is_err());
    }

    #[test]
    fn test_daemon_config_new_defaults_remote_domain_to_none() {
        let orig_remote = std::env::var("DEVVM_REMOTE_DOMAIN").ok();
        std::env::remove_var("DEVVM_REMOTE_DOMAIN");

        let config = DaemonConfig::new();
        assert_eq!(config.remote_domain, None);

        if let Some(r) = orig_remote {
            std::env::set_var("DEVVM_REMOTE_DOMAIN", r);
        }
    }
}
