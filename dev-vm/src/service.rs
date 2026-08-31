use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOS,
    Other,
}

impl Platform {
    pub fn current() -> Self {
        #[cfg(target_os = "linux")]
        {
            Platform::Linux
        }
        #[cfg(target_os = "macos")]
        {
            Platform::MacOS
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Platform::Other
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::MacOS => "macos",
            Platform::Other => "other",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug)]
pub enum ServiceError {
    Io(io::Error),
    UnsupportedPlatform(String),
    CommandFailed(String),
    NotInstalled,
    Other(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::Io(e) => write!(f, "I/O error: {}", e),
            ServiceError::UnsupportedPlatform(p) => write!(f, "Unsupported platform: {}", p),
            ServiceError::CommandFailed(msg) => write!(f, "Command failed: {}", msg),
            ServiceError::NotInstalled => write!(f, "Service is not installed"),
            ServiceError::Other(msg) => write!(f, "Service error: {}", msg),
        }
    }
}

impl std::error::Error for ServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ServiceError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ServiceError {
    fn from(e: io::Error) -> Self {
        ServiceError::Io(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceUnitConfig {
    pub bin_path: PathBuf,
    pub path_env: String,
    pub args: Vec<String>,
    pub description: String,
    pub working_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServicePlistConfig {
    pub label: String,
    pub bin_path: PathBuf,
    pub args: Vec<String>,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
    pub path_env: String,
    pub working_directory: Option<PathBuf>,
}

pub fn default_home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"))
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn systemd_quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quote = arg
        .chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\'' || c == '\\');
    if needs_quote {
        let escaped = arg.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    } else {
        arg.to_string()
    }
}

fn run_command_warn(cmd: &str, args: &[&str]) {
    match Command::new(cmd).args(args).output() {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let detail = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    format!("exit status {}", output.status)
                };
                tracing::warn!(
                    "Command '{} {}' failed ({}): {}",
                    cmd,
                    args.join(" "),
                    output.status,
                    detail
                );
            }
        }
        Err(e) => {
            tracing::warn!("Failed to execute '{} {}': {}", cmd, args.join(" "), e);
        }
    }
}

pub fn get_systemd_unit_dir(home_dir: &Path) -> PathBuf {
    home_dir.join(".config/systemd/user")
}

pub fn get_systemd_service_path(home_dir: &Path) -> PathBuf {
    get_systemd_unit_dir(home_dir).join("devvm-daemon.service")
}

pub fn get_launchd_agents_dir(home_dir: &Path) -> PathBuf {
    home_dir.join("Library/LaunchAgents")
}

pub fn get_launchd_plist_path(home_dir: &Path) -> PathBuf {
    get_launchd_agents_dir(home_dir).join("com.devvm.daemon.plist")
}

pub fn generate_systemd_unit(config: &ServiceUnitConfig) -> String {
    let mut exec_parts = vec![systemd_quote_arg(&config.bin_path.display().to_string())];
    for arg in &config.args {
        exec_parts.push(systemd_quote_arg(arg));
    }
    let exec_start = exec_parts.join(" ");

    let working_dir_line = match &config.working_directory {
        Some(dir) => format!("WorkingDirectory={}\n", dir.display()),
        None => String::new(),
    };

    format!(
        r#"[Unit]
Description={}
After=network.target

[Service]
Type=simple
ExecStart={}
Restart=on-failure
RestartSec=5
Environment=PATH={}
{}[Install]
WantedBy=default.target
"#,
        config.description, exec_start, config.path_env, working_dir_line
    )
}

pub fn generate_launchd_plist(config: &ServicePlistConfig) -> String {
    let mut args_xml = format!(
        "        <string>{}</string>\n",
        xml_escape(&config.bin_path.display().to_string())
    );
    for arg in &config.args {
        args_xml.push_str(&format!("        <string>{}</string>\n", xml_escape(arg)));
    }

    let working_dir_xml = match &config.working_directory {
        Some(dir) => format!(
            "    <key>WorkingDirectory</key>\n    <string>{}</string>\n",
            xml_escape(&dir.display().to_string())
        ),
        None => String::new(),
    };

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
{}    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
{}    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>{}</string>
    </dict>
</dict>
</plist>
"#,
        xml_escape(&config.label),
        args_xml,
        xml_escape(&config.stdout_path.display().to_string()),
        xml_escape(&config.stderr_path.display().to_string()),
        working_dir_xml,
        xml_escape(&config.path_env)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub platform: Platform,
    pub installed: bool,
    pub service_path: PathBuf,
    pub active: bool,
    pub details: String,
}

#[derive(Debug, Clone)]
pub struct ServiceManager {
    pub platform: Platform,
    pub home_dir: PathBuf,
    pub bin_path: PathBuf,
}

impl ServiceManager {
    pub fn new() -> Self {
        let home_dir = default_home_dir();
        let bin_path = home_dir.join(".local/bin/devvm-daemon");
        Self {
            platform: Platform::current(),
            home_dir,
            bin_path,
        }
    }

    pub fn with_custom(platform: Platform, home_dir: PathBuf, bin_path: PathBuf) -> Self {
        Self {
            platform,
            home_dir,
            bin_path,
        }
    }

    pub fn service_file_path(&self) -> PathBuf {
        match self.platform {
            Platform::Linux => get_systemd_service_path(&self.home_dir),
            Platform::MacOS => get_launchd_plist_path(&self.home_dir),
            Platform::Other => self.home_dir.join(".config/devvm-daemon.service"),
        }
    }

    pub fn install(
        &self,
        enable: bool,
        start: bool,
        extra_args: &[String],
    ) -> Result<PathBuf, ServiceError> {
        let path_env = std::env::var("PATH").unwrap_or_else(|_| {
            format!(
                "{}/.local/bin:/usr/local/bin:/usr/bin:/bin",
                self.home_dir.display()
            )
        });

        match self.platform {
            Platform::Linux => self.install_linux(path_env, enable, start, extra_args),
            Platform::MacOS => self.install_macos(path_env, enable, start, extra_args),
            Platform::Other => Err(ServiceError::UnsupportedPlatform(
                "Service management is only supported on Linux (systemd) and macOS (launchd)"
                    .to_string(),
            )),
        }
    }

    fn install_linux(
        &self,
        path_env: String,
        enable: bool,
        start: bool,
        extra_args: &[String],
    ) -> Result<PathBuf, ServiceError> {
        let service_path = get_systemd_service_path(&self.home_dir);
        if let Some(parent) = service_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut args = vec!["serve".to_string()];
        args.extend_from_slice(extra_args);

        let config = ServiceUnitConfig {
            bin_path: self.bin_path.clone(),
            path_env,
            args,
            description: "DevVM Workspace Supervision Control Daemon".to_string(),
            working_directory: Some(self.home_dir.clone()),
        };

        let unit_content = generate_systemd_unit(&config);
        fs::write(&service_path, unit_content)?;

        if enable || start {
            run_command_warn("systemctl", &["--user", "daemon-reload"]);
            if enable {
                run_command_warn("systemctl", &["--user", "enable", "devvm-daemon.service"]);
            }
            if start {
                run_command_warn("systemctl", &["--user", "start", "devvm-daemon.service"]);
            }
        }

        Ok(service_path)
    }

    fn install_macos(
        &self,
        path_env: String,
        enable: bool,
        start: bool,
        extra_args: &[String],
    ) -> Result<PathBuf, ServiceError> {
        let plist_path = get_launchd_plist_path(&self.home_dir);
        if let Some(parent) = plist_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let log_dir = self.home_dir.join(".local/share/devvm/logs");
        let _ = fs::create_dir_all(&log_dir);

        let mut args = vec!["serve".to_string()];
        args.extend_from_slice(extra_args);

        let config = ServicePlistConfig {
            label: "com.devvm.daemon".to_string(),
            bin_path: self.bin_path.clone(),
            args,
            stdout_path: log_dir.join("daemon.stdout.log"),
            stderr_path: log_dir.join("daemon.stderr.log"),
            path_env,
            working_directory: Some(self.home_dir.clone()),
        };

        let plist_content = generate_launchd_plist(&config);
        fs::write(&plist_path, plist_content)?;

        if start || enable {
            let plist_str = plist_path.display().to_string();
            run_command_warn("launchctl", &["load", &plist_str]);
        }

        Ok(plist_path)
    }

    pub fn uninstall(&self) -> Result<(), ServiceError> {
        let service_path = self.service_file_path();
        if !service_path.exists() {
            return Err(ServiceError::NotInstalled);
        }

        match self.platform {
            Platform::Linux => {
                run_command_warn("systemctl", &["--user", "stop", "devvm-daemon.service"]);
                run_command_warn("systemctl", &["--user", "disable", "devvm-daemon.service"]);
                fs::remove_file(&service_path)?;
                run_command_warn("systemctl", &["--user", "daemon-reload"]);
                Ok(())
            }
            Platform::MacOS => {
                let plist_str = service_path.display().to_string();
                run_command_warn("launchctl", &["unload", &plist_str]);
                fs::remove_file(&service_path)?;
                Ok(())
            }
            Platform::Other => Err(ServiceError::UnsupportedPlatform(
                "Service management is only supported on Linux (systemd) and macOS (launchd)"
                    .to_string(),
            )),
        }
    }

    pub fn status(&self) -> Result<ServiceStatus, ServiceError> {
        let service_path = self.service_file_path();
        if !service_path.exists() {
            return Ok(ServiceStatus {
                platform: self.platform,
                installed: false,
                service_path,
                active: false,
                details: "Service unit/plist is not installed".to_string(),
            });
        }

        match self.platform {
            Platform::Linux => self.status_linux(service_path),
            Platform::MacOS => self.status_macos(service_path),
            Platform::Other => Ok(ServiceStatus {
                platform: self.platform,
                installed: true,
                service_path,
                active: false,
                details: "Installed on unsupported platform".to_string(),
            }),
        }
    }

    fn status_linux(&self, service_path: PathBuf) -> Result<ServiceStatus, ServiceError> {
        let output = Command::new("systemctl")
            .args(["--user", "is-active", "devvm-daemon.service"])
            .output();

        let (is_active, details) = match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let active = stdout == "active";
                let details = if active {
                    "Service is installed and active (running)".to_string()
                } else if !stderr.is_empty() {
                    format!("Service is installed but not active ({})", stderr)
                } else {
                    format!("Service is installed but not active (state: {})", stdout)
                };
                (active, details)
            }
            Err(e) => {
                tracing::warn!("Failed to query systemctl is-active: {}", e);
                (false, format!("Failed to query service status: {}", e))
            }
        };

        Ok(ServiceStatus {
            platform: Platform::Linux,
            installed: true,
            service_path,
            active: is_active,
            details,
        })
    }

    fn status_macos(&self, service_path: PathBuf) -> Result<ServiceStatus, ServiceError> {
        let output = Command::new("launchctl")
            .args(["list", "com.devvm.daemon"])
            .output();

        let (is_active, details) = match output {
            Ok(out) => {
                let active = out.status.success();
                let details = if active {
                    "Launch agent is installed and loaded (active)".to_string()
                } else {
                    "Launch agent is installed but not loaded".to_string()
                };
                (active, details)
            }
            Err(e) => {
                tracing::warn!("Failed to query launchctl list: {}", e);
                (false, format!("Failed to query launchctl status: {}", e))
            }
        };

        Ok(ServiceStatus {
            platform: Platform::MacOS,
            installed: true,
            service_path,
            active: is_active,
            details,
        })
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsSetupInstructions {
    pub domain: String,
    pub port: u16,
    pub tailscale_ip: Option<String>,
    pub bin_path: PathBuf,
    pub linux_setcap_cmd: String,
    pub linux_resolved_path: PathBuf,
    pub linux_resolved_content: String,
    pub macos_resolver_path: PathBuf,
    pub macos_resolver_content: String,
    pub full_instructions: String,
}

pub fn generate_dns_setup_instructions(
    domain: &str,
    port: u16,
    tailscale_ip: Option<&str>,
    bin_path: Option<&Path>,
) -> DnsSetupInstructions {
    let resolved_bin = bin_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| default_home_dir().join(".local/bin/devvm-daemon"));

    let target_ip = tailscale_ip.unwrap_or("127.0.0.1");

    let linux_setcap_cmd = format!(
        "sudo setcap 'cap_net_bind_service=+ep' {}",
        resolved_bin.display()
    );
    let linux_resolved_path = PathBuf::from("/etc/systemd/resolved.conf.d/devvm.conf");
    let linux_resolved_content = format!(
        "[Resolve]\nDNS={}:{}\nDomains=~{}\n",
        target_ip, port, domain
    );

    let macos_resolver_path = PathBuf::from(format!("/etc/resolver/{}", domain));
    let macos_resolver_content = if port == 53 {
        format!("nameserver {}\n", target_ip)
    } else {
        format!("nameserver {}\nport {}\n", target_ip, port)
    };

    let full_instructions = format!(
        r#"=== DevVM Wildcard DNS Setup Instructions ===

Domain: *.{domain}
Target IP: {target_ip}
Port: {port}
Daemon binary: {bin_display}

1. Privileged Port 53 Capability (Linux only):
   To allow devvm-daemon to bind to port 53 without root:
     {linux_setcap_cmd}

2. Local Split DNS on Linux (systemd-resolved):
   Create {linux_resolved_path}:
----------------------------------------
{linux_resolved_content}----------------------------------------
   Apply changes:
     sudo systemctl restart systemd-resolved

3. Local Split DNS on macOS (/etc/resolver):
   Create {macos_resolver_path}:
----------------------------------------
{macos_resolver_content}----------------------------------------

4. Tailscale Private Network Split DNS:
   To make *.{domain} resolve from any device on your Tailnet:
   a. Open Tailscale Admin Console -> DNS -> Nameservers
   b. Click "Add Nameserver" -> "Custom"
   c. Domain: {domain}
   d. Nameserver IP: {target_ip} (your workstation Tailscale IP)
   e. Save changes.

Note: Normal daemon and CLI operations remain unprivileged.
"#,
        domain = domain,
        target_ip = target_ip,
        port = port,
        bin_display = resolved_bin.display(),
        linux_setcap_cmd = linux_setcap_cmd,
        linux_resolved_path = linux_resolved_path.display(),
        linux_resolved_content = linux_resolved_content,
        macos_resolver_path = macos_resolver_path.display(),
        macos_resolver_content = macos_resolver_content,
    );

    DnsSetupInstructions {
        domain: domain.to_string(),
        port,
        tailscale_ip: tailscale_ip.map(|s| s.to_string()),
        bin_path: resolved_bin,
        linux_setcap_cmd,
        linux_resolved_path,
        linux_resolved_content,
        macos_resolver_path,
        macos_resolver_content,
        full_instructions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_systemd_unit_generation() {
        let config = ServiceUnitConfig {
            bin_path: PathBuf::from("/home/user/.local/bin/devvm-daemon"),
            path_env: "/home/user/.local/bin:/usr/bin:/bin".to_string(),
            args: vec![
                "serve".to_string(),
                "--port".to_string(),
                "8100".to_string(),
            ],
            description: "DevVM Workspace Supervision Control Daemon".to_string(),
            working_directory: Some(PathBuf::from("/home/user")),
        };

        let unit = generate_systemd_unit(&config);
        assert!(unit.contains("Description=DevVM Workspace Supervision Control Daemon"));
        assert!(unit.contains("ExecStart=/home/user/.local/bin/devvm-daemon serve --port 8100"));
        assert!(unit.contains("Environment=PATH=/home/user/.local/bin:/usr/bin:/bin"));
        assert!(unit.contains("WorkingDirectory=/home/user"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn test_systemd_unit_multiword_and_special_char_args() {
        let config = ServiceUnitConfig {
            bin_path: PathBuf::from("/home/user with spaces/.local/bin/devvm-daemon"),
            path_env: "/usr/bin:/bin".to_string(),
            args: vec![
                "serve".to_string(),
                "--tailnet-domain".to_string(),
                "my host.internal".to_string(),
                "--custom-arg".to_string(),
                "foo \"bar\" \\baz".to_string(),
            ],
            description: "DevVM Workspace Supervision Control Daemon".to_string(),
            working_directory: Some(PathBuf::from("/home/user with spaces")),
        };

        let unit = generate_systemd_unit(&config);
        assert!(unit.contains("ExecStart=\"/home/user with spaces/.local/bin/devvm-daemon\" serve --tailnet-domain \"my host.internal\" --custom-arg \"foo \\\"bar\\\" \\\\baz\""));
        assert!(unit.contains("WorkingDirectory=/home/user with spaces"));
    }

    #[test]
    fn test_launchd_plist_generation() {
        let config = ServicePlistConfig {
            label: "com.devvm.daemon".to_string(),
            bin_path: PathBuf::from("/Users/user/.local/bin/devvm-daemon"),
            args: vec![
                "serve".to_string(),
                "--port".to_string(),
                "8100".to_string(),
            ],
            stdout_path: PathBuf::from("/Users/user/.local/share/devvm/logs/daemon.stdout.log"),
            stderr_path: PathBuf::from("/Users/user/.local/share/devvm/logs/daemon.stderr.log"),
            path_env: "/Users/user/.local/bin:/usr/bin:/bin".to_string(),
            working_directory: Some(PathBuf::from("/Users/user")),
        };

        let plist = generate_launchd_plist(&config);
        assert!(plist.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(plist.contains("<key>Label</key>\n    <string>com.devvm.daemon</string>"));
        assert!(plist.contains("<string>/Users/user/.local/bin/devvm-daemon</string>"));
        assert!(plist.contains("<string>serve</string>"));
        assert!(plist.contains("<string>--port</string>"));
        assert!(plist.contains("<string>8100</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(plist.contains("<key>KeepAlive</key>\n    <true/>"));
        assert!(plist.contains("<key>StandardOutPath</key>\n    <string>/Users/user/.local/share/devvm/logs/daemon.stdout.log</string>"));
        assert!(plist.contains("<key>StandardErrorPath</key>\n    <string>/Users/user/.local/share/devvm/logs/daemon.stderr.log</string>"));
        assert!(plist.contains("<key>WorkingDirectory</key>\n    <string>/Users/user</string>"));
    }

    #[test]
    fn test_launchd_plist_xml_escaping() {
        let config = ServicePlistConfig {
            label: "com.devvm.daemon&test".to_string(),
            bin_path: PathBuf::from("/path/to/bin"),
            args: vec!["<foo>".to_string(), "\"quoted\"".to_string()],
            stdout_path: PathBuf::from("/path/stdout.log"),
            stderr_path: PathBuf::from("/path/stderr.log"),
            path_env: "/bin".to_string(),
            working_directory: None,
        };

        let plist = generate_launchd_plist(&config);
        assert!(plist.contains("<string>com.devvm.daemon&amp;test</string>"));
        assert!(plist.contains("<string>&lt;foo&gt;</string>"));
        assert!(plist.contains("<string>&quot;quoted&quot;</string>"));
    }

    #[test]
    fn test_service_manager_linux_lifecycle() {
        let temp = tempdir().unwrap();
        let home = temp.path().to_path_buf();
        let bin_path = home.join(".local/bin/devvm-daemon");

        let manager = ServiceManager::with_custom(Platform::Linux, home.clone(), bin_path.clone());

        // Status before install
        let status = manager.status().unwrap();
        assert!(!status.installed);
        assert!(!status.active);

        // Install
        let path = manager
            .install(false, false, &["--port".to_string(), "9000".to_string()])
            .unwrap();
        assert_eq!(path, home.join(".config/systemd/user/devvm-daemon.service"));
        assert!(path.exists());

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("devvm-daemon serve --port 9000"));

        // Status after install
        let status = manager.status().unwrap();
        assert!(status.installed);

        // Uninstall
        manager.uninstall().unwrap();
        assert!(!path.exists());

        let status = manager.status().unwrap();
        assert!(!status.installed);
    }

    #[test]
    fn test_service_manager_macos_lifecycle() {
        let temp = tempdir().unwrap();
        let home = temp.path().to_path_buf();
        let bin_path = home.join(".local/bin/devvm-daemon");

        let manager = ServiceManager::with_custom(Platform::MacOS, home.clone(), bin_path.clone());

        // Status before install
        let status = manager.status().unwrap();
        assert!(!status.installed);
        assert!(!status.active);

        // Install
        let path = manager.install(false, false, &[]).unwrap();
        assert_eq!(
            path,
            home.join("Library/LaunchAgents/com.devvm.daemon.plist")
        );
        assert!(path.exists());

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("<string>com.devvm.daemon</string>"));

        // Status after install
        let status = manager.status().unwrap();
        assert!(status.installed);

        // Uninstall
        manager.uninstall().unwrap();
        assert!(!path.exists());

        let status = manager.status().unwrap();
        assert!(!status.installed);
    }

    #[test]
    fn test_dns_setup_instructions_generation() {
        let instructions = generate_dns_setup_instructions(
            "devvm.internal",
            53,
            Some("100.64.0.5"),
            Some(Path::new("/custom/bin/devvm-daemon")),
        );

        assert_eq!(instructions.domain, "devvm.internal");
        assert_eq!(instructions.port, 53);
        assert_eq!(instructions.tailscale_ip, Some("100.64.0.5".to_string()));
        assert!(instructions
            .linux_setcap_cmd
            .contains("/custom/bin/devvm-daemon"));
        assert!(instructions
            .linux_resolved_content
            .contains("DNS=100.64.0.5:53"));
        assert!(instructions
            .linux_resolved_content
            .contains("Domains=~devvm.internal"));
        assert!(instructions
            .macos_resolver_content
            .contains("nameserver 100.64.0.5"));
        assert!(instructions
            .full_instructions
            .contains("Tailscale Admin Console"));
    }
}
