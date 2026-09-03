use clap::{Parser, Subcommand};
use devvm_daemon::{
    create_router, default_home_dir, detect_tailscale_ipv4, determine_bind_addresses,
    generate_dns_setup_instructions, provision_sync_setup, AppState, DaemonConfig, DnsConfig,
    DnsServer, DshRuntimeManager, Platform, ServiceManager, SyncConfig, SyncManager,
};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(
    name = "devvm-daemon",
    about = "DevVM Workspace Supervision Control Daemon & DNS Server"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub serve_args: ServeArgs,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Commands {
    /// Run the Control Daemon HTTP API and Web UI (default)
    Serve(ServeArgs),
    /// Manage user service (systemd on Linux, launchd on macOS)
    Service(ServiceArgs),
    /// Run or configure the wildcard DNS server for devvm.internal
    Dns(DnsArgs),
    /// Manage Portable DSH State synchronization
    Sync(SyncArgs),
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub command: ServiceCommands,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum ServiceCommands {
    /// Install user service (systemd user on Linux, launchd on macOS)
    Install(ServiceInstallArgs),
    /// Uninstall user service
    Uninstall(ServiceUninstallArgs),
    /// View user service status
    Status(ServiceStatusArgs),
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct ServiceInstallArgs {
    #[arg(long)]
    pub enable: bool,

    #[arg(long)]
    pub start: bool,

    #[arg(long)]
    pub bin_path: Option<PathBuf>,

    #[arg(long)]
    pub home_dir: Option<PathBuf>,

    #[arg(long)]
    pub port: Option<u16>,

    #[arg(long)]
    pub host: Option<String>,

    #[arg(long)]
    pub ingress_port: Option<u16>,

    #[arg(long)]
    pub tailnet_domain: Option<String>,
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct ServiceUninstallArgs {
    #[arg(long)]
    pub home_dir: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatusArgs {
    #[arg(long)]
    pub home_dir: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub command: SyncCommands,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum SyncCommands {
    /// Configure and verify Sync Store credentials
    Setup(SyncSetupArgs),
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct SyncSetupArgs {
    #[arg(long, env = "DEVVM_SYNC_SSH_USER")]
    pub ssh_user: String,

    #[arg(long, env = "DEVVM_SYNC_SSH_HOST")]
    pub ssh_host: String,

    #[arg(long, env = "DEVVM_SYNC_SSH_PORT", default_value_t = 22)]
    pub ssh_port: u16,

    #[arg(long, env = "DEVVM_SYNC_SSH_KEY")]
    pub ssh_key: PathBuf,

    #[arg(
        long,
        env = "DEVVM_SYNC_REMOTE_ROOT",
        default_value = "/var/lib/devvm-sync"
    )]
    pub remote_root: String,

    #[arg(long, env = "DEVVM_SYNC_CONFIG_PATH")]
    pub config_path: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    pub no_verify: bool,

    #[arg(long, env = "DEVVM_PORT")]
    pub port: Option<u16>,
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct ServeArgs {
    #[arg(long, env = "DEVVM_HOST")]
    pub host: Option<String>,

    #[arg(long, env = "DEVVM_PORT", default_value_t = 8100)]
    pub port: u16,

    #[arg(long, env = "DEVVM_CONFIG_PATH")]
    pub config: Option<PathBuf>,

    #[arg(long, env = "DEVVM_LOG_DIR")]
    pub log_dir: Option<PathBuf>,

    #[arg(long, env = "DEVVM_HOME_DIR")]
    pub home_dir: Option<PathBuf>,

    #[arg(long, env = "DEVVM_BIN")]
    pub devvm_bin: Option<PathBuf>,

    #[arg(long, env = "DEVVM_INGRESS_PORT", default_value_t = 8102)]
    pub ingress_port: u16,

    #[arg(long, env = "DEVVM_TAILNET_DOMAIN", default_value = "devvm.internal")]
    pub tailnet_domain: String,
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct DnsArgs {
    #[command(subcommand)]
    pub command: Option<DnsCommands>,

    #[arg(
        long,
        short = 'b',
        env = "DEVVM_DNS_BIND",
        default_value = "0.0.0.0:53"
    )]
    pub bind: String,

    #[arg(long, short = 'i', env = "DEVVM_DNS_IP")]
    pub ip: Option<String>,

    #[arg(
        long,
        short = 'd',
        env = "DEVVM_DNS_DOMAIN",
        default_value = "devvm.internal"
    )]
    pub domain: String,

    #[arg(long, env = "DEVVM_DNS_IPV6")]
    pub ipv6: Option<String>,

    #[arg(long, env = "DEVVM_DNS_TTL", default_value_t = 60)]
    pub ttl: u32,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum DnsCommands {
    /// Configure and generate wildcard DNS setup instructions
    Setup(DnsSetupArgs),
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct DnsSetupArgs {
    #[arg(long, default_value_t = 53)]
    pub port: u16,

    #[arg(long, env = "DEVVM_TAILSCALE_IP")]
    pub tailscale_ip: Option<String>,

    #[arg(long, default_value = "devvm.internal")]
    pub domain: String,

    #[arg(long)]
    pub bin_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "devvm_daemon=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Service(service_args)) => {
            run_service_command(service_args)?;
        }
        Some(Commands::Dns(dns_args)) => match dns_args.command {
            Some(DnsCommands::Setup(setup_args)) => {
                run_dns_setup(setup_args)?;
            }
            None => {
                let target_ip: Ipv4Addr = match dns_args.ip {
                    Some(ref ip_str) => ip_str.parse()?,
                    None => detect_tailscale_ipv4().unwrap_or_else(|| Ipv4Addr::new(127, 0, 0, 1)),
                };
                let target_ipv6 = match dns_args.ipv6 {
                    Some(s) => Some(s.parse::<Ipv6Addr>()?),
                    None => None,
                };
                let dns_config = DnsConfig {
                    bind_addr: dns_args.bind,
                    target_ip,
                    domain: dns_args.domain,
                    target_ipv6,
                    ttl: dns_args.ttl,
                };
                let server = DnsServer::new(dns_config);
                server.run().await?;
            }
        },
        Some(Commands::Sync(sync_args)) => match sync_args.command {
            SyncCommands::Setup(setup_args) => {
                run_sync_setup(setup_args).await?;
            }
        },
        Some(Commands::Serve(serve_args)) => {
            run_daemon(serve_args).await?;
        }
        None => {
            run_daemon(cli.serve_args).await?;
        }
    }

    Ok(())
}

fn run_service_command(args: ServiceArgs) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        ServiceCommands::Install(install_args) => {
            let home_dir = install_args
                .home_dir
                .or_else(dirs::home_dir)
                .unwrap_or_else(default_home_dir);

            let bin_path = install_args.bin_path.unwrap_or_else(|| {
                std::env::current_exe().unwrap_or_else(|_| home_dir.join(".local/bin/devvm-daemon"))
            });

            let mut extra_args = Vec::new();
            if let Some(port) = install_args.port {
                extra_args.push("--port".to_string());
                extra_args.push(port.to_string());
            }
            if let Some(host) = install_args.host {
                extra_args.push("--host".to_string());
                extra_args.push(host);
            }
            if let Some(ingress_port) = install_args.ingress_port {
                extra_args.push("--ingress-port".to_string());
                extra_args.push(ingress_port.to_string());
            }
            if let Some(domain) = install_args.tailnet_domain {
                extra_args.push("--tailnet-domain".to_string());
                extra_args.push(domain);
            }

            let manager = ServiceManager::with_custom(Platform::current(), home_dir, bin_path);
            let path = manager.install(install_args.enable, install_args.start, &extra_args)?;
            println!("Installed service to {}", path.display());
            if install_args.enable {
                println!("Service enabled.");
            }
            if install_args.start {
                println!("Service started.");
            }
        }
        ServiceCommands::Uninstall(uninstall_args) => {
            let home_dir = uninstall_args
                .home_dir
                .or_else(dirs::home_dir)
                .unwrap_or_else(default_home_dir);
            let bin_path = home_dir.join(".local/bin/devvm-daemon");

            let manager = ServiceManager::with_custom(Platform::current(), home_dir, bin_path);
            manager.uninstall()?;
            println!("Service uninstalled successfully.");
        }
        ServiceCommands::Status(status_args) => {
            let home_dir = status_args
                .home_dir
                .or_else(dirs::home_dir)
                .unwrap_or_else(default_home_dir);
            let bin_path = home_dir.join(".local/bin/devvm-daemon");

            let manager = ServiceManager::with_custom(Platform::current(), home_dir, bin_path);
            let status = manager.status()?;
            println!("Platform: {}", status.platform);
            println!("Installed: {}", if status.installed { "yes" } else { "no" });
            println!("Service file: {}", status.service_path.display());
            println!(
                "Active: {}",
                if status.active { "yes (running)" } else { "no" }
            );
            println!("Details: {}", status.details);
        }
    }
    Ok(())
}

fn run_dns_setup(args: DnsSetupArgs) -> Result<(), Box<dyn std::error::Error>> {
    let detected_ip = detect_tailscale_ipv4().map(|ip| ip.to_string());
    let tailscale_ip = args.tailscale_ip.as_deref().or(detected_ip.as_deref());
    let instructions = generate_dns_setup_instructions(
        &args.domain,
        args.port,
        tailscale_ip,
        args.bin_path.as_deref(),
    );
    println!("{}", instructions.full_instructions);
    Ok(())
}

async fn run_sync_setup(args: SyncSetupArgs) -> Result<(), Box<dyn std::error::Error>> {
    let sync_config = SyncConfig {
        ssh_user: args.ssh_user,
        ssh_host: args.ssh_host,
        ssh_port: args.ssh_port,
        ssh_key_path: args.ssh_key,
        remote_sync_root: args.remote_root,
        writer_id: None,
        daemon_url: None,
    };

    let sync_manager = SyncManager::new();
    if !args.no_verify {
        println!(
            "Verifying SSH connectivity with Sync Store at {}@{}:{}...",
            sync_config.ssh_user, sync_config.ssh_host, sync_config.ssh_port
        );
        if let Err(e) = sync_manager.verify(&sync_config).await {
            eprintln!("Verification failed: {}", e);
            return Err(e.into());
        }
        println!("Connectivity verified successfully.");
    }

    let config_path = args.config_path.unwrap_or_else(|| {
        let home = default_home_dir();
        home.join(".config/devvm/sync.json")
    });

    let port = args.port.unwrap_or_else(|| DaemonConfig::new().port);
    let daemon_url = format!("http://127.0.0.1:{}", port);

    provision_sync_setup(&config_path, &sync_config, &daemon_url)?;
    println!("Sync configuration saved to {}", config_path.display());
    Ok(())
}

async fn run_daemon(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = DaemonConfig::new();
    config.host = args.host.clone().unwrap_or_default();
    config.port = args.port;
    config.ingress_port = args.ingress_port;
    config.tailnet_domain = args.tailnet_domain;
    if let Some(c) = args.config {
        config.config_path = c;
    }
    if let Some(l) = args.log_dir {
        config.log_dir = l;
    }
    if let Some(h) = args.home_dir {
        config.home_dir = h;
    }
    if let Some(d) = args.devvm_bin {
        config.devvm_bin = d;
    }

    let dsh_runtime_manager = DshRuntimeManager::new();
    let sync_manager = SyncManager::with_devvm_bin(config.devvm_bin.clone());
    let state = AppState {
        config: config.clone(),
        dsh_runtime_manager,
        sync_manager,
    };

    let app = create_router(state);

    let ts_ip = detect_tailscale_ipv4();
    let bind_addrs = determine_bind_addresses(args.host.as_deref(), config.port, ts_ip);

    let mut listeners = Vec::new();
    for addr in bind_addrs {
        tracing::info!("Starting DevVM Control Daemon on http://{}", addr);
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listeners.push((addr, listener)),
            Err(e) => {
                if addr.ip().is_loopback() || listeners.is_empty() {
                    return Err(format!("Failed to bind to {}: {}", addr, e).into());
                } else {
                    tracing::warn!("Failed to bind to secondary address {}: {}", addr, e);
                }
            }
        }
    }

    if listeners.len() == 1 {
        let (_, listener) = listeners.into_iter().next().unwrap();
        axum::serve(listener, app).await?;
    } else {
        let mut set = tokio::task::JoinSet::new();
        for (addr, listener) in listeners {
            let app_clone = app.clone();
            set.spawn(async move {
                tracing::info!("DevVM Control Daemon listening on http://{}", addr);
                if let Err(e) = axum::serve(listener, app_clone).await {
                    tracing::error!("Server on {} failed: {}", addr, e);
                }
            });
        }
        while let Some(res) = set.join_next().await {
            res?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_default_serve_args() {
        let cli = Cli::try_parse_from(["devvm-daemon"]).unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.serve_args.port, 8100);
        assert_eq!(cli.serve_args.host, None);
        assert_eq!(cli.serve_args.ingress_port, 8102);
        assert_eq!(cli.serve_args.tailnet_domain, "devvm.internal");
    }

    #[test]
    fn test_cli_serve_subcommand() {
        let cli = Cli::try_parse_from([
            "devvm-daemon",
            "serve",
            "--port",
            "9100",
            "--ingress-port",
            "9102",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Serve(args)) => {
                assert_eq!(args.port, 9100);
                assert_eq!(args.host, None);
                assert_eq!(args.ingress_port, 9102);
            }
            _ => panic!("Expected Serve subcommand"),
        }
    }

    #[test]
    fn test_cli_serve_subcommand_with_host() {
        let cli = Cli::try_parse_from([
            "devvm-daemon",
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            "9100",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Serve(args)) => {
                assert_eq!(args.port, 9100);
                assert_eq!(args.host, Some("127.0.0.1".to_string()));
            }
            _ => panic!("Expected Serve subcommand"),
        }
    }

    #[test]
    fn test_cli_service_install_subcommand() {
        let cli = Cli::try_parse_from([
            "devvm-daemon",
            "service",
            "install",
            "--enable",
            "--start",
            "--port",
            "8100",
            "--host",
            "127.0.0.1",
            "--ingress-port",
            "8102",
        ])
        .unwrap();

        match cli.command {
            Some(Commands::Service(ServiceArgs {
                command: ServiceCommands::Install(args),
            })) => {
                assert!(args.enable);
                assert!(args.start);
                assert_eq!(args.port, Some(8100));
                assert_eq!(args.host, Some("127.0.0.1".to_string()));
                assert_eq!(args.ingress_port, Some(8102));
            }
            _ => panic!("Expected Service Install subcommand"),
        }
    }

    #[test]
    fn test_cli_service_uninstall_subcommand() {
        let cli = Cli::try_parse_from(["devvm-daemon", "service", "uninstall"]).unwrap();
        match cli.command {
            Some(Commands::Service(ServiceArgs {
                command: ServiceCommands::Uninstall(_),
            })) => {}
            _ => panic!("Expected Service Uninstall subcommand"),
        }
    }

    #[test]
    fn test_cli_service_status_subcommand() {
        let cli = Cli::try_parse_from(["devvm-daemon", "service", "status"]).unwrap();
        match cli.command {
            Some(Commands::Service(ServiceArgs {
                command: ServiceCommands::Status(_),
            })) => {}
            _ => panic!("Expected Service Status subcommand"),
        }
    }

    #[test]
    fn test_cli_dns_subcommand_serve() {
        let cli = Cli::try_parse_from([
            "devvm-daemon",
            "dns",
            "--bind",
            "127.0.0.1:1053",
            "--ip",
            "100.64.0.10",
            "--domain",
            "devvm.internal",
            "--ttl",
            "300",
        ])
        .unwrap();

        match cli.command {
            Some(Commands::Dns(args)) => {
                assert!(args.command.is_none());
                assert_eq!(args.bind, "127.0.0.1:1053");
                assert_eq!(args.ip, Some("100.64.0.10".to_string()));
                assert_eq!(args.domain, "devvm.internal");
                assert_eq!(args.ttl, 300);
            }
            _ => panic!("Expected Dns subcommand"),
        }
    }

    #[test]
    fn test_cli_dns_subcommand_default_args() {
        let cli = Cli::try_parse_from(["devvm-daemon", "dns"]).unwrap();
        match cli.command {
            Some(Commands::Dns(args)) => {
                assert!(args.command.is_none());
                assert_eq!(args.bind, "0.0.0.0:53");
                assert_eq!(args.ip, None);
                assert_eq!(args.domain, "devvm.internal");
                assert_eq!(args.ttl, 60);
            }
            _ => panic!("Expected Dns subcommand"),
        }
    }

    #[test]
    fn test_cli_dns_subcommand_setup() {
        let cli = Cli::try_parse_from([
            "devvm-daemon",
            "dns",
            "setup",
            "--port",
            "53",
            "--tailscale-ip",
            "100.64.0.10",
            "--domain",
            "devvm.internal",
        ])
        .unwrap();

        match cli.command {
            Some(Commands::Dns(args)) => match args.command {
                Some(DnsCommands::Setup(setup_args)) => {
                    assert_eq!(setup_args.port, 53);
                    assert_eq!(setup_args.tailscale_ip, Some("100.64.0.10".to_string()));
                    assert_eq!(setup_args.domain, "devvm.internal");
                }
                None => panic!("Expected Dns setup subcommand"),
            },
            _ => panic!("Expected Dns subcommand"),
        }
    }

    #[test]
    fn test_cli_sync_setup_subcommand() {
        let cli = Cli::try_parse_from([
            "devvm-daemon",
            "sync",
            "setup",
            "--ssh-user",
            "ubuntu",
            "--ssh-host",
            "vps.devvm.net",
            "--ssh-port",
            "2222",
            "--ssh-key",
            "/root/.ssh/id_rsa",
            "--remote-root",
            "/var/lib/sync-store",
            "--no-verify",
        ])
        .unwrap();

        match cli.command {
            Some(Commands::Sync(SyncArgs {
                command: SyncCommands::Setup(args),
            })) => {
                assert_eq!(args.ssh_user, "ubuntu");
                assert_eq!(args.ssh_host, "vps.devvm.net");
                assert_eq!(args.ssh_port, 2222);
                assert_eq!(args.ssh_key, PathBuf::from("/root/.ssh/id_rsa"));
                assert_eq!(args.remote_root, "/var/lib/sync-store");
                assert!(args.no_verify);
            }
            _ => panic!("Expected Sync Setup subcommand"),
        }
    }
}
