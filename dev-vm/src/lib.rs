pub mod api;
pub mod browser;
pub mod config;
pub mod dns;
pub mod logs;
pub mod models;
pub mod registry;
pub mod runner;
pub mod runtime;
pub mod service;
pub mod sync;
pub mod ui;

pub use api::{create_router, AppState};
pub use config::{determine_bind_addresses, DaemonConfig};
pub use dns::{detect_tailscale_ipv4, DnsConfig, DnsServer};
pub use runtime::DshRuntimeManager;
pub use service::{
    default_home_dir, generate_dns_setup_instructions, generate_launchd_plist,
    generate_systemd_unit, get_launchd_plist_path, get_systemd_service_path, DnsSetupInstructions,
    Platform, ServiceError, ServiceManager, ServicePlistConfig, ServiceStatus, ServiceUnitConfig,
};
pub use sync::{
    apply_rsync_filters, check_local_portable_state_exists, is_local_state_dirty, load_sync_config,
    mark_local_state_clean, mark_local_state_dirty, provision_sync_setup,
    resolve_host_ssh_key_path, save_sync_config, SyncConfig, SyncDirection, SyncError, SyncManager,
    SyncRunner, SystemSyncRunner,
};
