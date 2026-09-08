use clap::{Parser, Subcommand};
use devvm_daemon::{
    create_router, default_home_dir, determine_bind_address, provision_sync_setup, AppState,
    DaemonConfig, DshRuntimeManager, Platform, ServiceManager, SyncConfig, SyncManager,
};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(
    name = "devvm-daemon",
    about = "DevVM Workspace Supervision Control Daemon"
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
    pub remote_domain: Option<String>,
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

    #[arg(long, env = "DEVVM_REMOTE_DOMAIN")]
    pub remote_domain: Option<String>,
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
            if let Some(domain) = install_args.remote_domain {
                devvm_daemon::validate_domain(&domain)?;
                extra_args.push("--remote-domain".to_string());
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
    if let Some(ref d) = args.remote_domain {
        config.remote_domain = Some(d.clone());
    }
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

    if let Some(domain) = &config.remote_domain {
        devvm_daemon::validate_domain(domain)?;
    }
    let dsh_runtime_manager = DshRuntimeManager::new();
    let sync_manager = SyncManager::with_devvm_bin(config.devvm_bin.clone());
    let state = AppState {
        config: config.clone(),
        dsh_runtime_manager,
        sync_manager,
    };

    let app = create_router(state);

    let addr = determine_bind_address(args.host.as_deref(), config.port)?;
    tracing::info!("Starting DevVM Control Daemon on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

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
        assert_eq!(cli.serve_args.remote_domain, None);
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
                assert_eq!(args.remote_domain, None);
            }
            _ => panic!("Expected Serve subcommand"),
        }
    }

    #[test]
    fn test_cli_serve_subcommand_with_remote_domain() {
        let cli =
            Cli::try_parse_from(["devvm-daemon", "serve", "--remote-domain", "risak.dev"]).unwrap();
        match cli.command {
            Some(Commands::Serve(args)) => {
                assert_eq!(args.remote_domain, Some("risak.dev".to_string()));
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
            "--remote-domain",
            "risak.dev",
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
                assert_eq!(args.remote_domain, Some("risak.dev".to_string()));
            }
            _ => panic!("Expected Service Install subcommand"),
        }
    }

    #[test]
    fn test_run_service_command_install_with_remote_domain() {
        let temp = tempfile::tempdir().unwrap();
        let home_dir = temp.path().to_path_buf();
        let bin_path = home_dir.join(".local/bin/devvm-daemon");

        let args = ServiceArgs {
            command: ServiceCommands::Install(ServiceInstallArgs {
                enable: false,
                start: false,
                bin_path: Some(bin_path),
                home_dir: Some(home_dir.clone()),
                port: Some(8100),
                host: Some("127.0.0.1".to_string()),
                ingress_port: Some(8102),
                remote_domain: Some("risak.dev".to_string()),
            }),
        };

        run_service_command(args).unwrap();

        let unit_file = home_dir.join(".config/systemd/user/devvm-daemon.service");
        let plist_file = home_dir.join("Library/LaunchAgents/com.devvm.daemon.plist");
        let content = if unit_file.exists() {
            std::fs::read_to_string(unit_file).unwrap()
        } else if plist_file.exists() {
            std::fs::read_to_string(plist_file).unwrap()
        } else {
            panic!("Service file not created");
        };

        assert!(content.contains("--remote-domain"));
        assert!(content.contains("risak.dev"));
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
