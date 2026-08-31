use crate::service::default_home_dir;
use std::net::{Ipv4Addr, SocketAddr};
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
    pub tailnet_domain: String,
}

pub fn determine_bind_addresses(
    host: Option<&str>,
    port: u16,
    tailscale_ip: Option<Ipv4Addr>,
) -> Vec<SocketAddr> {
    match host {
        Some(h) if !h.trim().is_empty() => {
            if let Ok(ip) = h.parse::<std::net::IpAddr>() {
                vec![SocketAddr::new(ip, port)]
            } else {
                vec![format!("{}:{}", h, port)
                    .parse()
                    .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], port)))]
            }
        }
        _ => {
            let mut addrs = vec![SocketAddr::from(([127, 0, 0, 1], port))];
            if let Some(ts_ip) = tailscale_ip {
                if !ts_ip.is_loopback() {
                    let ts_addr = SocketAddr::new(std::net::IpAddr::V4(ts_ip), port);
                    if !addrs.contains(&ts_addr) {
                        addrs.push(ts_addr);
                    }
                }
            }
            addrs
        }
    }
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

        let log_dir = std::env::var("DEVVM_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::data_local_dir()
                    .map(|p| p.join("devvm/logs"))
                    .unwrap_or_else(|| home_dir.join(".local/share/devvm/logs"))
            });

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

        let tailnet_domain =
            std::env::var("DEVVM_TAILNET_DOMAIN").unwrap_or_else(|_| "devvm.internal".to_string());

        Self {
            host,
            port,
            config_path,
            sync_config_path,
            log_dir,
            home_dir,
            devvm_bin,
            ingress_port,
            tailnet_domain,
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
    fn test_determine_bind_addresses_default_no_tailscale() {
        let addrs = determine_bind_addresses(None, 8100, None);
        assert_eq!(addrs, vec![SocketAddr::from(([127, 0, 0, 1], 8100))]);
    }

    #[test]
    fn test_determine_bind_addresses_default_with_tailscale() {
        let ts_ip: Ipv4Addr = "100.64.0.5".parse().unwrap();
        let addrs = determine_bind_addresses(None, 8100, Some(ts_ip));
        assert_eq!(
            addrs,
            vec![
                SocketAddr::from(([127, 0, 0, 1], 8100)),
                SocketAddr::from(([100, 64, 0, 5], 8100))
            ]
        );
    }

    #[test]
    fn test_determine_bind_addresses_empty_string_host_with_tailscale() {
        let ts_ip: Ipv4Addr = "100.64.0.5".parse().unwrap();
        let addrs = determine_bind_addresses(Some(""), 8100, Some(ts_ip));
        assert_eq!(
            addrs,
            vec![
                SocketAddr::from(([127, 0, 0, 1], 8100)),
                SocketAddr::from(([100, 64, 0, 5], 8100))
            ]
        );
    }

    #[test]
    fn test_determine_bind_addresses_explicit_host() {
        let ts_ip: Ipv4Addr = "100.64.0.5".parse().unwrap();
        let addrs = determine_bind_addresses(Some("127.0.0.1"), 8100, Some(ts_ip));
        assert_eq!(addrs, vec![SocketAddr::from(([127, 0, 0, 1], 8100))]);
    }

    #[test]
    fn test_determine_bind_addresses_explicit_custom_ip() {
        let addrs = determine_bind_addresses(Some("192.168.1.50"), 9000, None);
        assert_eq!(addrs, vec![SocketAddr::from(([192, 168, 1, 50], 9000))]);
    }

    #[test]
    fn test_determine_bind_addresses_no_wildcard_0_0_0_0() {
        let addrs = determine_bind_addresses(None, 8100, None);
        assert!(!addrs.iter().any(|a| a.ip().is_unspecified()));

        let ts_ip: Ipv4Addr = "100.64.0.5".parse().unwrap();
        let addrs_ts = determine_bind_addresses(None, 8100, Some(ts_ip));
        assert!(!addrs_ts.iter().any(|a| a.ip().is_unspecified()));
    }
}
