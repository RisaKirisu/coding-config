use devvm_daemon::{
    generate_dns_setup_instructions, generate_launchd_plist, generate_systemd_unit,
    get_launchd_plist_path, get_systemd_service_path, Platform, ServiceManager, ServicePlistConfig,
    ServiceUnitConfig,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_systemd_unit_content_generation() {
    let bin_path = PathBuf::from("/home/alice/.local/bin/devvm-daemon");
    let path_env = "/home/alice/.local/bin:/usr/local/bin:/usr/bin:/bin".to_string();
    let config = ServiceUnitConfig {
        bin_path: bin_path.clone(),
        path_env: path_env.clone(),
        args: vec![
            "serve".to_string(),
            "--port".to_string(),
            "8200".to_string(),
            "--ingress-port".to_string(),
            "8202".to_string(),
        ],
        description: "DevVM Workspace Supervision Control Daemon".to_string(),
        working_directory: Some(PathBuf::from("/home/alice")),
    };

    let unit = generate_systemd_unit(&config);

    assert!(unit.contains("[Unit]"));
    assert!(unit.contains("Description=DevVM Workspace Supervision Control Daemon"));
    assert!(unit.contains("After=network.target"));
    assert!(unit.contains("[Service]"));
    assert!(unit.contains("Type=simple"));
    assert!(unit.contains(
        "ExecStart=/home/alice/.local/bin/devvm-daemon serve --port 8200 --ingress-port 8202"
    ));
    assert!(unit.contains("Restart=on-failure"));
    assert!(unit.contains("RestartSec=5"));
    assert!(unit.contains(&format!("Environment=PATH={}", path_env)));
    assert!(unit.contains("WorkingDirectory=/home/alice"));
    assert!(unit.contains("[Install]"));
    assert!(unit.contains("WantedBy=default.target"));
}

#[test]
fn test_systemd_unit_special_characters_quoting() {
    let bin_path = PathBuf::from("/opt/devvm/bin directory/devvm-daemon");
    let config = ServiceUnitConfig {
        bin_path,
        path_env: "/usr/bin:/bin".to_string(),
        args: vec![
            "serve".to_string(),
            "--tailnet-domain".to_string(),
            "custom domain.internal".to_string(),
            "--extra".to_string(),
            "val with \"quotes\" and \\slash".to_string(),
        ],
        description: "DevVM Daemon".to_string(),
        working_directory: Some(PathBuf::from("/opt/devvm")),
    };

    let unit = generate_systemd_unit(&config);
    assert!(unit.contains("ExecStart=\"/opt/devvm/bin directory/devvm-daemon\" serve --tailnet-domain \"custom domain.internal\" --extra \"val with \\\"quotes\\\" and \\\\slash\""));
}

#[test]
fn test_launchd_plist_xml_content_generation() {
    let bin_path = PathBuf::from("/Users/bob/.local/bin/devvm-daemon");
    let log_dir = PathBuf::from("/Users/bob/.local/share/devvm/logs");
    let config = ServicePlistConfig {
        label: "com.devvm.daemon".to_string(),
        bin_path: bin_path.clone(),
        args: vec![
            "serve".to_string(),
            "--port".to_string(),
            "8100".to_string(),
            "--tailnet-domain".to_string(),
            "devvm.internal".to_string(),
        ],
        stdout_path: log_dir.join("daemon.stdout.log"),
        stderr_path: log_dir.join("daemon.stderr.log"),
        path_env: "/Users/bob/.local/bin:/usr/local/bin:/usr/bin:/bin".to_string(),
        working_directory: Some(PathBuf::from("/Users/bob")),
    };

    let plist = generate_launchd_plist(&config);

    assert!(plist.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(plist.contains("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">"));
    assert!(plist.contains("<plist version=\"1.0\">"));
    assert!(plist.contains("<dict>"));
    assert!(plist.contains("<key>Label</key>\n    <string>com.devvm.daemon</string>"));
    assert!(plist.contains("<key>ProgramArguments</key>"));
    assert!(plist.contains("<string>/Users/bob/.local/bin/devvm-daemon</string>"));
    assert!(plist.contains("<string>serve</string>"));
    assert!(plist.contains("<string>--port</string>"));
    assert!(plist.contains("<string>8100</string>"));
    assert!(plist.contains("<string>--tailnet-domain</string>"));
    assert!(plist.contains("<string>devvm.internal</string>"));
    assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
    assert!(plist.contains("<key>KeepAlive</key>\n    <true/>"));
    assert!(plist.contains("<key>StandardOutPath</key>\n    <string>/Users/bob/.local/share/devvm/logs/daemon.stdout.log</string>"));
    assert!(plist.contains("<key>StandardErrorPath</key>\n    <string>/Users/bob/.local/share/devvm/logs/daemon.stderr.log</string>"));
    assert!(plist.contains("<key>WorkingDirectory</key>\n    <string>/Users/bob</string>"));
    assert!(plist.contains("<key>EnvironmentVariables</key>"));
    assert!(plist.contains("<key>PATH</key>\n        <string>/Users/bob/.local/bin:/usr/local/bin:/usr/bin:/bin</string>"));
    assert!(plist.ends_with("</dict>\n</plist>\n"));
}

#[test]
fn test_launchd_plist_special_characters_escaping() {
    let config = ServicePlistConfig {
        label: "com.devvm.daemon<>&\"'".to_string(),
        bin_path: PathBuf::from("/opt/devvm/bin & tools/devvm-daemon"),
        args: vec!["--param=\"foo & bar <baz>'\"".to_string()],
        stdout_path: PathBuf::from("/tmp/out & err.log"),
        stderr_path: PathBuf::from("/tmp/err <1>.log"),
        path_env: "/bin:/usr/bin".to_string(),
        working_directory: None,
    };

    let plist = generate_launchd_plist(&config);

    assert!(plist.contains("com.devvm.daemon&lt;&gt;&amp;&quot;&apos;"));
    assert!(plist.contains("/opt/devvm/bin &amp; tools/devvm-daemon"));
    assert!(plist.contains("--param=&quot;foo &amp; bar &lt;baz&gt;&apos;&quot;"));
    assert!(plist.contains("/tmp/out &amp; err.log"));
    assert!(plist.contains("/tmp/err &lt;1&gt;.log"));
}

#[test]
fn test_service_manager_linux_fixture() {
    let temp = tempdir().unwrap();
    let home = temp.path().to_path_buf();
    let bin = home.join(".local/bin/devvm-daemon");

    let manager = ServiceManager::with_custom(Platform::Linux, home.clone(), bin.clone());

    // Initially uninstalled
    let status_before = manager.status().unwrap();
    assert!(!status_before.installed);
    assert_eq!(status_before.platform, Platform::Linux);
    assert_eq!(status_before.service_path, get_systemd_service_path(&home));

    // Install
    let installed_path = manager
        .install(false, false, &["--port".to_string(), "9100".to_string()])
        .unwrap();
    assert_eq!(installed_path, get_systemd_service_path(&home));
    assert!(installed_path.exists());

    let content = fs::read_to_string(&installed_path).unwrap();
    assert!(content.contains("devvm-daemon serve --port 9100"));
    assert!(content.contains(&format!("WorkingDirectory={}", home.display())));

    // Status is installed
    let status_installed = manager.status().unwrap();
    assert!(status_installed.installed);

    // Uninstall
    manager.uninstall().unwrap();
    assert!(!installed_path.exists());

    // Status is uninstalled
    let status_uninstalled = manager.status().unwrap();
    assert!(!status_uninstalled.installed);
}

#[test]
fn test_service_manager_macos_fixture() {
    let temp = tempdir().unwrap();
    let home = temp.path().to_path_buf();
    let bin = home.join(".local/bin/devvm-daemon");

    let manager = ServiceManager::with_custom(Platform::MacOS, home.clone(), bin.clone());

    // Initially uninstalled
    let status_before = manager.status().unwrap();
    assert!(!status_before.installed);
    assert_eq!(status_before.platform, Platform::MacOS);
    assert_eq!(status_before.service_path, get_launchd_plist_path(&home));

    // Install
    let installed_path = manager
        .install(
            false,
            false,
            &[
                "--tailnet-domain".to_string(),
                "custom.internal".to_string(),
            ],
        )
        .unwrap();
    assert_eq!(installed_path, get_launchd_plist_path(&home));
    assert!(installed_path.exists());

    let content = fs::read_to_string(&installed_path).unwrap();
    assert!(content.contains("<string>custom.internal</string>"));
    assert!(content.contains(&format!("<string>{}</string>", home.display())));

    // Status is installed
    let status_installed = manager.status().unwrap();
    assert!(status_installed.installed);

    // Uninstall
    manager.uninstall().unwrap();
    assert!(!installed_path.exists());

    // Status is uninstalled
    let status_uninstalled = manager.status().unwrap();
    assert!(!status_uninstalled.installed);
}

#[test]
fn test_dns_setup_helper_generation() {
    let instructions = generate_dns_setup_instructions(
        "devvm.internal",
        53,
        Some("100.64.0.42"),
        Some(Path::new("/home/user/.local/bin/devvm-daemon")),
    );

    assert_eq!(instructions.domain, "devvm.internal");
    assert_eq!(instructions.port, 53);
    assert_eq!(instructions.tailscale_ip.as_deref(), Some("100.64.0.42"));
    assert_eq!(
        instructions.bin_path,
        PathBuf::from("/home/user/.local/bin/devvm-daemon")
    );

    // Linux setcap command
    assert_eq!(
        instructions.linux_setcap_cmd,
        "sudo setcap 'cap_net_bind_service=+ep' /home/user/.local/bin/devvm-daemon"
    );

    // Linux resolved config
    assert_eq!(
        instructions.linux_resolved_content,
        "[Resolve]\nDNS=100.64.0.42:53\nDomains=~devvm.internal\n"
    );

    // macOS resolver config
    assert_eq!(
        instructions.macos_resolver_content,
        "nameserver 100.64.0.42\n"
    );

    // Full instructions text includes key sections
    assert!(instructions
        .full_instructions
        .contains("=== DevVM Wildcard DNS Setup Instructions ==="));
    assert!(instructions
        .full_instructions
        .contains("1. Privileged Port 53 Capability (Linux only):"));
    assert!(instructions
        .full_instructions
        .contains("2. Local Split DNS on Linux (systemd-resolved):"));
    assert!(instructions
        .full_instructions
        .contains("3. Local Split DNS on macOS (/etc/resolver):"));
    assert!(instructions
        .full_instructions
        .contains("4. Tailscale Private Network Split DNS:"));
    assert!(instructions
        .full_instructions
        .contains("Note: Normal daemon and CLI operations remain unprivileged."));
}

#[test]
fn test_dns_setup_helper_non_standard_port() {
    let instructions =
        generate_dns_setup_instructions("devvm.internal", 1053, Some("127.0.0.1"), None);

    assert_eq!(instructions.port, 1053);
    assert_eq!(
        instructions.macos_resolver_content,
        "nameserver 127.0.0.1\nport 1053\n"
    );
    assert_eq!(
        instructions.linux_resolved_content,
        "[Resolve]\nDNS=127.0.0.1:1053\nDomains=~devvm.internal\n"
    );
}

#[test]
fn test_script_syntax_and_dry_run() {
    // 1. Verify syntax of setup-devvm.sh
    let status = Command::new("bash")
        .args(["-n", "setup-devvm.sh"])
        .status()
        .expect("Failed to run bash syntax check on setup-devvm.sh");
    assert!(status.success(), "setup-devvm.sh has bash syntax errors");

    // 2. Verify setup-devvm.sh --help
    let output = Command::new("bash")
        .args(["setup-devvm.sh", "--help"])
        .output()
        .expect("Failed to run setup-devvm.sh --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--service"));
    assert!(stdout.contains("--skip-image"));

    // 3. Verify syntax of scripts/setup-dns.sh
    let status = Command::new("bash")
        .args(["-n", "scripts/setup-dns.sh"])
        .status()
        .expect("Failed to run bash syntax check on scripts/setup-dns.sh");
    assert!(
        status.success(),
        "scripts/setup-dns.sh has bash syntax errors"
    );

    // 4. Verify scripts/setup-dns.sh --dry-run
    let output = Command::new("bash")
        .args(["scripts/setup-dns.sh", "--dry-run"])
        .output()
        .expect("Failed to run scripts/setup-dns.sh --dry-run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== DevVM One-Time DNS Setup ==="));
    assert!(stdout.contains("Tailscale Split DNS"));
}

#[test]
fn test_service_installation_with_custom_host_and_default_host() {
    let temp = tempdir().unwrap();
    let home = temp.path().to_path_buf();
    let bin_path = home.join(".local/bin/devvm-daemon");

    // 1. Default install (no --host argument): unit runs `devvm-daemon serve --port ...`
    // which binds to 127.0.0.1 + Tailscale IP without 0.0.0.0
    let manager = ServiceManager::with_custom(Platform::Linux, home.clone(), bin_path.clone());
    let path = manager
        .install(false, false, &["--port".to_string(), "8100".to_string()])
        .unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("devvm-daemon serve --port 8100"));
    assert!(!content.contains("--host"));

    // 2. Custom host install (with --host argument): unit preserves explicit --host
    let path_custom = manager
        .install(
            false,
            false,
            &[
                "--port".to_string(),
                "8100".to_string(),
                "--host".to_string(),
                "127.0.0.1".to_string(),
            ],
        )
        .unwrap();
    let content_custom = fs::read_to_string(&path_custom).unwrap();
    assert!(content_custom.contains("devvm-daemon serve --port 8100 --host 127.0.0.1"));
}

#[test]
fn test_devvm_start_and_exec_invokes_profile_dependency_installation_in_order() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = tempdir().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    let smolvm_log = temp_dir.path().join("smolvm.log");

    // Fake smolvm script that logs all invocations
    let smolvm_script = format!(
        r#"#!/usr/bin/env bash
echo "===SMOLVM_INVOCATION===" >> "{0}"
printf '%s\n' "$@" >> "{0}"
if [[ "$1" == "machine" && "$2" == "status" ]]; then
    exit 0
fi
exit 0
"#,
        smolvm_log.display()
    );
    let smolvm_bin = bin_dir.join("smolvm");
    fs::write(&smolvm_bin, smolvm_script).unwrap();
    let mut perms = fs::metadata(&smolvm_bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&smolvm_bin, perms).unwrap();

    // Fake frps script
    let frps_bin = bin_dir.join("frps");
    fs::write(&frps_bin, "#!/usr/bin/env bash\nsleep 10\n").unwrap();
    let mut frps_perms = fs::metadata(&frps_bin).unwrap().permissions();
    frps_perms.set_mode(0o755);
    fs::set_permissions(&frps_bin, frps_perms).unwrap();

    // Mock devvm home & project
    let devvm_home = temp_dir.path().join("devvm_home");
    fs::create_dir_all(devvm_home.join("root/.dsh/profiles/web")).unwrap();
    fs::write(devvm_home.join("root/.dsh/profiles/web/package.json"), "{}").unwrap();
    fs::write(devvm_home.join("smolvm.toml"), "# smolvm").unwrap();

    let proj_dir = temp_dir.path().join("my-project");
    fs::create_dir_all(&proj_dir).unwrap();
    fs::write(
        proj_dir.join(".devvm-id"),
        "00000000-0000-0000-0000-000000000001\n",
    )
    .unwrap();

    let devvm_script = Path::new(env!("CARGO_MANIFEST_DIR")).join("devvm");

    let path_env = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    // 1. Test `devvm start`
    let output_start = Command::new(&devvm_script)
        .arg("start")
        .current_dir(&proj_dir)
        .env("PATH", &path_env)
        .env("DEVVM_HOME", &devvm_home)
        .env("DEVVM_ROOT", devvm_home.join("root"))
        .env("FRPS_BIN", &frps_bin)
        .output()
        .expect("Failed to run devvm start");
    assert!(
        output_start.status.success(),
        "devvm start failed: {:?}",
        output_start
    );

    let logs_start = fs::read_to_string(&smolvm_log).unwrap();
    let invocations_start: Vec<&str> = logs_start
        .split("===SMOLVM_INVOCATION===\n")
        .filter(|s| !s.trim().is_empty())
        .collect();

    // Verify ordering in devvm start:
    // a. machine start
    let start_idx = invocations_start
        .iter()
        .position(|l| l.contains("machine\nstart") || l.contains("machine start"))
        .expect("must invoke machine start");
    // b. ingress
    let ingress_idx = invocations_start
        .iter()
        .position(|l| l.contains("devvm-ingress"))
        .expect("must invoke devvm-ingress");
    // c. link_root
    let link_idx = invocations_start
        .iter()
        .position(|l| l.contains("link-root"))
        .expect("must invoke link-root");

    assert!(
        start_idx < ingress_idx,
        "machine start must precede ingress"
    );
    assert!(ingress_idx < link_idx, "ingress must precede link-root");

    let link_command = invocations_start[link_idx];
    assert!(
        link_command.contains("dsh plugin --profile web install --frozen-lockfile"),
        "link-root must execute dsh plugin install: {}",
        link_command
    );
    assert!(
        link_command.contains("DSH_HOME=/root/.dsh"),
        "link-root must set DSH_HOME=/root/.dsh: {}",
        link_command
    );
    assert!(
        link_command.contains("/root/.dsh/sessions /root/.dsh/storages /root/.dsh/attachments"),
        "link-root must create VM-local Portable DSH State directories: {}",
        link_command
    );
    assert!(
        link_command.contains("$name\" == \"attachments"),
        "link-root must not link host attachments into the DevVM: {}",
        link_command
    );

    // Verify that inside link_root script, symlink setup occurs before dsh install
    let link_pos = link_command
        .find("/root/.dsh/$name")
        .expect("must find dsh link target");
    let install_pos = link_command
        .find("dsh plugin --profile web install")
        .expect("must find dsh install");
    assert!(
        link_pos < install_pos,
        "link setup must occur before dsh plugin install"
    );

    // 2. Test `devvm exec`
    fs::write(&smolvm_log, "").unwrap(); // clear log
    let output_exec = Command::new(&devvm_script)
        .args(["exec", "--", "echo", "final_guest_payload"])
        .current_dir(&proj_dir)
        .env("PATH", &path_env)
        .env("DEVVM_HOME", &devvm_home)
        .env("DEVVM_ROOT", devvm_home.join("root"))
        .env("FRPS_BIN", &frps_bin)
        .output()
        .expect("Failed to run devvm exec");
    assert!(
        output_exec.status.success(),
        "devvm exec failed: {:?}",
        output_exec
    );

    let logs_exec = fs::read_to_string(&smolvm_log).unwrap();
    let invocations_exec: Vec<&str> = logs_exec
        .split("===SMOLVM_INVOCATION===\n")
        .filter(|s| !s.trim().is_empty())
        .collect();

    let link_exec_idx = invocations_exec
        .iter()
        .position(|l| l.contains("link-root"))
        .expect("devvm exec must run link-root");
    let final_cmd_idx = invocations_exec
        .iter()
        .position(|l| l.contains("final_guest_payload"))
        .expect("devvm exec must run final payload");

    assert!(
        link_exec_idx < final_cmd_idx,
        "link-root (with profile dependency install) must execute before the final user command"
    );
}
