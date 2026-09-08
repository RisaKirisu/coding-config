use devvm_daemon::{
    generate_launchd_plist, generate_systemd_unit, get_launchd_plist_path,
    get_systemd_service_path, Platform, ServiceManager, ServicePlistConfig, ServiceUnitConfig,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_web_profile_links_first_party_plugins_to_their_sources() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let profile_path = manifest_dir.join("root/.dsh/profiles/web/package.json");
    let profile: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&profile_path).unwrap()).unwrap();
    let dependencies = profile["dependencies"].as_object().unwrap();
    let node_modules = manifest_dir.join("root/.dsh/profiles/web/node_modules");

    for (package, source) in [
        ("@devvm/dsh-build-loop", "build-loop"),
        ("@devvm/dsh-remote-sync", "remote-sync"),
        ("@devvm/dsh-style-control", "style-control"),
        ("@devvm/dsh-subagent-manager", "subagent-manager"),
        ("@devvm/dsh-voice-input", "voice-input"),
        ("dsh-skill-mcp-panel", "dsh-skill-mcp-panel"),
    ] {
        assert_eq!(
            dependencies.get(package).and_then(|value| value.as_str()),
            Some(format!("link:/root/.dsh/plugins/{source}").as_str()),
            "{package} manifest spec must be link:/root/.dsh/plugins/{source}"
        );

        let installed_path = node_modules.join(package);
        let metadata = fs::symlink_metadata(&installed_path).unwrap_or_else(|e| {
            panic!(
                "failed to read metadata for {}: {e}",
                installed_path.display()
            )
        });
        assert!(
            metadata.file_type().is_symlink(),
            "{package} at {} must be a symbolic link rather than a hardlinked directory",
            installed_path.display()
        );

        let target = fs::canonicalize(&installed_path)
            .unwrap_or_else(|e| panic!("failed to canonicalize {}: {e}", installed_path.display()));
        let expected_target = fs::canonicalize(manifest_dir.join("root/.dsh/plugins").join(source))
            .unwrap_or_else(|e| panic!("failed to canonicalize source for {source}: {e}"));
        assert_eq!(
            target, expected_target,
            "{package} symlink must resolve to its source directory"
        );
    }
}

#[test]
fn test_profiles_do_not_declare_duplicate_web_fetch() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for profile in ["web", "headless"] {
        let profile_path = repo_root.join(format!("root/.dsh/profiles/{profile}/package.json"));
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&profile_path).unwrap()).unwrap();
        if let Some(deps) = manifest.get("dependencies").and_then(|d| d.as_object()) {
            assert!(
                !deps.contains_key("@deepseek-ai/dsh-web-fetch-http"),
                "{profile} profile must not declare @deepseek-ai/dsh-web-fetch-http because DSH base includes it"
            );
        }
        if let Some(bundles) = manifest
            .get("dsh")
            .and_then(|d| d.get("profile"))
            .and_then(|p| p.get("bundles"))
            .and_then(|b| b.as_array())
        {
            assert!(
                !bundles
                    .iter()
                    .any(|b| b.as_str() == Some("@deepseek-ai/dsh-web-fetch-http")),
                "{profile} profile must not declare @deepseek-ai/dsh-web-fetch-http in bundles"
            );
        }
    }
}

#[test]
fn test_web_profile_third_party_plugin_pins() {
    let profile_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("root/.dsh/profiles/web/package.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&profile_path).unwrap()).unwrap();
    let deps = manifest["dependencies"].as_object().unwrap();
    assert_eq!(
        deps.get("@hytime/dsh-thinking-effort")
            .and_then(|v| v.as_str()),
        Some("^0.2.0")
    );
    assert_eq!(
        deps.get("dsh-better-sidebar").and_then(|v| v.as_str()),
        Some("^0.18.0")
    );
}

#[test]
fn test_web_profile_lockfile_has_no_file_links_for_local_plugins() {
    let lockfile_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("root/.dsh/profiles/web/pnpm-lock.yaml");
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    for plugin in [
        "build-loop",
        "remote-sync",
        "style-control",
        "subagent-manager",
        "voice-input",
        "dsh-skill-mcp-panel",
    ] {
        assert!(
            !lockfile_content.contains(&format!("file:/root/.dsh/plugins/{plugin}")),
            "lockfile must not contain file: specifier for {plugin}"
        );
        assert!(
            !lockfile_content.contains(&format!("file:../../../../root/.dsh/plugins/{plugin}")),
            "lockfile must not contain file: version for {plugin}"
        );
        assert!(
            lockfile_content.contains(&format!("specifier: link:/root/.dsh/plugins/{plugin}")),
            "lockfile must contain link: specifier for {plugin}"
        );
        assert!(
            lockfile_content.contains(&format!("version: link:../../plugins/{plugin}")),
            "lockfile must contain link: version for {plugin}"
        );
    }

    assert!(
        lockfile_content.contains("'@hytime/dsh-thinking-effort@0.2.0':"),
        "lockfile must contain @hytime/dsh-thinking-effort locked at 0.2.0"
    );
    assert!(
        !lockfile_content.contains("38f541073e7193d940a9ab5295cf8eb1e5ad5d6d"),
        "lockfile must not contain obsolete GitHub commit for thinking effort"
    );
}

#[test]
fn test_headless_profile_lockfile_does_not_contain_duplicate_web_fetch() {
    let lockfile_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("root/.dsh/profiles/headless/pnpm-lock.yaml");
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    assert!(
        !lockfile_content.contains("@deepseek-ai/dsh-web-fetch-http"),
        "headless lockfile must not contain @deepseek-ai/dsh-web-fetch-http"
    );
}

#[test]
fn test_plugins_directory_has_fallback_node_modules_symlink() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let link_path = manifest_dir.join("root/.dsh/plugins/node_modules");
    let metadata = fs::symlink_metadata(&link_path)
        .unwrap_or_else(|e| panic!("failed to read metadata for {}: {e}", link_path.display()));
    assert!(
        metadata.file_type().is_symlink(),
        "{} must be a symbolic link",
        link_path.display()
    );
    let target = fs::read_link(&link_path).unwrap_or_else(|e| {
        panic!(
            "failed to read link target for {}: {e}",
            link_path.display()
        )
    });
    assert_eq!(
        target,
        Path::new("../profiles/node_modules"),
        "plugins/node_modules must point to ../profiles/node_modules"
    );
    let canonical = fs::canonicalize(&link_path)
        .unwrap_or_else(|e| panic!("failed to canonicalize {}: {e}", link_path.display()));
    let expected = fs::canonicalize(manifest_dir.join("root/.dsh/profiles/node_modules"))
        .unwrap_or_else(|e| panic!("failed to canonicalize profiles/node_modules: {e}"));
    assert_eq!(canonical, expected);
}

#[test]
fn test_systemd_unit_content_generation() {
    let bin_path = PathBuf::from("/home/alice/.local/bin/devvm-daemon");
    let path_env =
        "/home/alice/.local/bin:/mnt/c/Program Files/Tailscale:/usr/bin:/bin".to_string();
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
    assert!(unit.contains(&format!("Environment=\"PATH={}\"", path_env)));
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
    assert!(plist.contains("<key>Label</key>"));
    assert!(plist.contains("<string>com.devvm.daemon</string>"));
    assert!(plist.contains("<key>ProgramArguments</key>"));
    assert!(plist.contains("<string>/Users/bob/.local/bin/devvm-daemon</string>"));
    assert!(plist.contains("<string>serve</string>"));
    assert!(plist.contains("<string>--port</string>"));
    assert!(plist.contains("<string>8100</string>"));
    assert!(plist.contains("<string>--tailnet-domain</string>"));
    assert!(plist.contains("<string>devvm.internal</string>"));
    assert!(plist.contains("<key>RunAtLoad</key>"));
    assert!(plist.contains("<key>KeepAlive</key>"));
    assert!(plist.contains("<true/>"));
    assert!(plist.contains("<key>StandardOutPath</key>"));
    assert!(plist.contains("<string>/Users/bob/.local/share/devvm/logs/daemon.stdout.log</string>"));
    assert!(plist.contains("<key>StandardErrorPath</key>"));
    assert!(plist.contains("<string>/Users/bob/.local/share/devvm/logs/daemon.stderr.log</string>"));
    assert!(plist.contains("<key>WorkingDirectory</key>"));
    assert!(plist.contains("<string>/Users/bob</string>"));
    assert!(plist.contains("<key>EnvironmentVariables</key>"));
    assert!(plist.contains("<key>PATH</key>"));
    assert!(plist.contains("<string>/Users/bob/.local/bin:/usr/local/bin:/usr/bin:/bin</string>"));
    assert!(plist.contains("</plist>"));
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
    assert!(stdout.contains("--remote"));
    assert!(stdout.contains("--remote-domain"));
    assert!(stdout.contains("--remote-ip"));
    assert!(stdout.contains("--skip-image"));

    // 3. Verify override flags without --remote are rejected
    let override_err = Command::new("bash")
        .args(["setup-devvm.sh", "--remote-domain", "custom.dev"])
        .output()
        .expect("Failed to run setup-devvm.sh with invalid flags");
    assert!(!override_err.status.success());
    let stderr = String::from_utf8_lossy(&override_err.stderr);
    assert!(stderr.contains("require --remote"));

    let ip_override_err = Command::new("bash")
        .args(["setup-devvm.sh", "--remote-ip", "100.64.0.1"])
        .output()
        .expect("Failed to run setup-devvm.sh with invalid flags");
    assert!(!ip_override_err.status.success());
    let stderr = String::from_utf8_lossy(&ip_override_err.stderr);
    assert!(stderr.contains("require --remote"));

    // 4. Verify scripts/Caddyfile.host exists and contains expected directives
    let host_caddyfile =
        fs::read_to_string("scripts/Caddyfile.host").expect("scripts/Caddyfile.host must exist");
    assert!(host_caddyfile.contains("devvm.{$REMOTE_DOMAIN:risak.dev}"));
    assert!(host_caddyfile.contains("bind {$REMOTE_IP:100.67.154.69}"));
    assert!(host_caddyfile.contains("dns cloudflare {env.CLOUDFLARE_API_TOKEN}"));
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
fn test_devvm_start_is_idempotent_and_exec_reuses_running_machine() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = tempdir().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    let smolvm_log = temp_dir.path().join("smolvm.log");
    let smolvm_running = temp_dir.path().join("smolvm.running");
    let lifecycle_events = temp_dir.path().join("lifecycle-events.log");

    // Fake external CLI: expose machine state and observable dispatched operations.
    let smolvm_script = format!(
        r#"#!/usr/bin/env bash
printf '%s\n' "$*" >> "{0}"
if [[ "$1" == "machine" && "$2" == "status" ]]; then
    if [[ -f "{1}" ]]; then
        echo "running"
    else
        echo "stopped"
    fi
    exit 0
fi
if [[ "$1" == "machine" && "$2" == "create" ]]; then
    echo "machine-create" >> "{2}"
elif [[ "$1" == "machine" && "$2" == "start" ]]; then
    touch "{1}"
    echo "machine-start" >> "{2}"
elif [[ "$*" == *"devvm-ingress"* ]]; then
    echo "ingress-start" >> "{2}"
elif [[ "$*" == *"link-root"* ]]; then
    if [[ "$*" != *"CI=true DSH_HOME=/root/.dsh"* ]]; then
        echo "[ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY] Aborted removal of modules directory due to no TTY" >&2
        exit 1
    fi
    echo "profile-prepare" >> "{2}"
fi
exit 0
"#,
        smolvm_log.display(),
        smolvm_running.display(),
        lifecycle_events.display()
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

    assert!(
        smolvm_running.exists(),
        "start must leave the machine running"
    );
    let first_events = fs::read_to_string(&lifecycle_events).unwrap();
    assert_eq!(
        first_events
            .lines()
            .filter(|line| *line == "machine-create")
            .count(),
        0
    );
    assert_eq!(
        first_events
            .lines()
            .filter(|line| *line == "machine-start")
            .count(),
        1
    );
    assert_eq!(
        first_events
            .lines()
            .filter(|line| *line == "ingress-start")
            .count(),
        1
    );
    assert_eq!(
        first_events
            .lines()
            .filter(|line| *line == "profile-prepare")
            .count(),
        1
    );

    // 2. Repeated start is a no-op once the machine is running.
    let events_before_repeat = fs::read_to_string(&lifecycle_events).unwrap();
    let output_start_again = Command::new(&devvm_script)
        .arg("start")
        .current_dir(&proj_dir)
        .env("PATH", &path_env)
        .env("DEVVM_HOME", &devvm_home)
        .env("DEVVM_ROOT", devvm_home.join("root"))
        .env("FRPS_BIN", &frps_bin)
        .output()
        .expect("Failed to rerun devvm start");
    assert!(output_start_again.status.success());
    assert_eq!(
        fs::read_to_string(&lifecycle_events).unwrap(),
        events_before_repeat,
        "repeated start must not dispatch lifecycle side effects"
    );

    // 3. Regular exec reuses the running machine and dispatches only the payload.
    fs::write(&smolvm_log, "").unwrap();
    let events_before_exec = fs::read_to_string(&lifecycle_events).unwrap();
    let output_exec = Command::new(&devvm_script)
        .args(["exec", "--", "echo", "running_guest_payload"])
        .current_dir(&proj_dir)
        .env("PATH", &path_env)
        .env("DEVVM_HOME", &devvm_home)
        .env("DEVVM_ROOT", devvm_home.join("root"))
        .env("FRPS_BIN", &frps_bin)
        .output()
        .expect("Failed to run devvm exec");
    assert!(
        output_exec.status.success(),
        "devvm exec failed: {output_exec:?}"
    );

    let running_log = fs::read_to_string(&smolvm_log).unwrap();
    assert!(running_log
        .lines()
        .any(|line| line.ends_with("echo running_guest_payload")));
    assert_eq!(
        fs::read_to_string(&lifecycle_events).unwrap(),
        events_before_exec,
        "exec on a running machine must not dispatch lifecycle side effects"
    );
}

#[test]
fn test_setup_installs_upgrades_and_skips_current_versions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let devvm_home = temp.path().join("devvm");
    let fake_bin = temp.path().join("bin");
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    fs::create_dir_all(&devvm_home).unwrap();
    fs::create_dir_all(&fake_bin).unwrap();

    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("setup-devvm.sh"),
        devvm_home.join("setup-devvm.sh"),
    )
    .unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("devvm"),
        devvm_home.join("devvm"),
    )
    .unwrap();
    fs::create_dir_all(devvm_home.join("scripts")).unwrap();
    fs::copy(
        "scripts/Caddyfile.host",
        devvm_home.join("scripts/Caddyfile.host"),
    )
    .unwrap();
    fs::write(devvm_home.join("smolvm.toml"), "# test\n").unwrap();

    let frps = home.join(".local/bin/frps");
    fs::write(&frps, "#!/usr/bin/env bash\nexit 0\n").unwrap();
    fs::set_permissions(&frps, fs::Permissions::from_mode(0o755)).unwrap();

    let install_log = temp.path().join("smolvm-installs.log");
    let curl = fake_bin.join("curl");
    fs::write(
        &curl,
        r#"#!/usr/bin/env bash
if [[ "$*" == *"github.com/smol-machines/smolvm/releases/latest"* ]]; then
    printf 'https://github.com/smol-machines/smolvm/releases/tag/v1.13.1'
elif [[ "$*" == *"smolmachines.com/install.sh"* ]]; then
    cat <<'INSTALLER'
#!/usr/bin/env bash
set -e
version=""
while [[ $# -gt 0 ]]; do
    if [[ "$1" == "--version" ]]; then version="$2"; shift 2; else shift; fi
done
mkdir -p "$HOME/.smolvm" "$HOME/.local/bin"
printf '%s\n' "$version" > "$HOME/.smolvm/.version"
printf '#!/usr/bin/env bash\nprintf "smolvm %s\\n"\n' "$version" > "$HOME/.smolvm/smolvm"
chmod +x "$HOME/.smolvm/smolvm"
ln -sfn "$HOME/.smolvm/smolvm" "$HOME/.local/bin/smolvm"
printf '%s\n' "$version" >> "$FAKE_SMOLVM_INSTALL_LOG"
INSTALLER
else
    exit 1
fi
"#,
    )
    .unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();

    let cargo = fake_bin.join("cargo");
    fs::write(
        &cargo,
        r#"#!/usr/bin/env bash
set -e
mkdir -p "$DEVVM_HOME/target/release"
printf '%s' "$FAKE_DAEMON_BUILD" > "$DEVVM_HOME/target/release/devvm-daemon"
chmod +x "$DEVVM_HOME/target/release/devvm-daemon"
"#,
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();

    let path = format!(
        "{}:{}:{}",
        home.join(".local/bin").display(),
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let setup = devvm_home.join("setup-devvm.sh");
    let run_setup = |daemon_build: &str| {
        Command::new(&setup)
            .arg("--skip-image")
            .current_dir(&devvm_home)
            .env("HOME", &home)
            .env("DEVVM_HOME", &devvm_home)
            .env("PATH", &path)
            .env("FAKE_SMOLVM_INSTALL_LOG", &install_log)
            .env("FAKE_DAEMON_BUILD", daemon_build)
            .output()
            .unwrap()
    };

    let first = run_setup("daemon-v1");
    assert!(first.status.success(), "first setup failed: {first:?}");
    assert_eq!(
        fs::read_to_string(home.join(".smolvm/.version"))
            .unwrap()
            .trim(),
        "1.13.1"
    );
    assert_eq!(
        fs::read_to_string(home.join(".local/bin/devvm-daemon")).unwrap(),
        "daemon-v1"
    );

    let second = run_setup("daemon-v1");
    assert!(second.status.success(), "second setup failed: {second:?}");
    assert_eq!(fs::read_to_string(&install_log).unwrap().lines().count(), 1);
    assert_eq!(
        fs::read_to_string(home.join(".smolvm/.version"))
            .unwrap()
            .trim(),
        "1.13.1"
    );
    assert_eq!(
        fs::read_to_string(home.join(".local/bin/devvm-daemon")).unwrap(),
        "daemon-v1"
    );

    fs::write(home.join(".smolvm/.version"), "1.12.0\n").unwrap();
    let upgrade = run_setup("daemon-v2");
    assert!(
        upgrade.status.success(),
        "upgrade setup failed: {upgrade:?}"
    );
    assert_eq!(fs::read_to_string(&install_log).unwrap().lines().count(), 2);
    assert_eq!(
        fs::read_to_string(home.join(".local/bin/devvm-daemon")).unwrap(),
        "daemon-v2"
    );

    // Service setup: ordinary --service is local-only; --remote opts into remote HTTPS
    let command_log = temp.path().join("service-commands.log");
    for (name, body) in [
        (
            "systemctl",
            "#!/usr/bin/env bash\nprintf 'systemctl %s\\n' \"$*\" >> \"$FAKE_SERVICE_COMMAND_LOG\"\n",
        ),
        ("sudo", "#!/usr/bin/env bash\nexec \"$@\"\n"),
    ] {
        let path = fake_bin.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let daemon_script = format!(
        "#!/usr/bin/env bash\nprintf 'daemon %s\\n' \"$*\" >> '{}'\n",
        command_log.display()
    );

    // 1. Local-only service setup
    let serviced_local = Command::new(&setup)
        .args(["--skip-image", "--service"])
        .current_dir(&devvm_home)
        .env("HOME", &home)
        .env("DEVVM_HOME", &devvm_home)
        .env("PATH", &path)
        .env("FAKE_SMOLVM_INSTALL_LOG", &install_log)
        .env("FAKE_DAEMON_BUILD", &daemon_script)
        .env("FAKE_SERVICE_COMMAND_LOG", &command_log)
        .output()
        .unwrap();
    assert!(
        serviced_local.status.success(),
        "local service setup failed: {serviced_local:?}"
    );

    let commands_local = fs::read_to_string(&command_log).unwrap();
    assert!(commands_local.contains("daemon service install --enable"));
    assert!(!commands_local.contains("--remote-domain"));
    assert!(commands_local.contains("systemctl --user restart devvm-daemon.service"));
    assert!(!home
        .join(".config/systemd/user/devvm-daemon-dns.service")
        .exists());
    assert!(!devvm_home
        .join("root/.config/devvm/Caddyfile.host")
        .exists());

    // Clear command log for next run
    fs::write(&command_log, "").unwrap();

    // 2. Opt-in remote service setup with defaults (risak.dev, 100.67.154.69)
    let serviced_remote = Command::new(&setup)
        .args(["--skip-image", "--service", "--remote"])
        .current_dir(&devvm_home)
        .env("HOME", &home)
        .env("DEVVM_HOME", &devvm_home)
        .env("PATH", &path)
        .env("FAKE_SMOLVM_INSTALL_LOG", &install_log)
        .env("FAKE_DAEMON_BUILD", &daemon_script)
        .env("FAKE_SERVICE_COMMAND_LOG", &command_log)
        .output()
        .unwrap();
    assert!(
        serviced_remote.status.success(),
        "remote service setup failed: {serviced_remote:?}"
    );

    let commands_remote = fs::read_to_string(&command_log).unwrap();
    assert!(commands_remote.contains("daemon service install --enable --remote-domain risak.dev"));
    let output = String::from_utf8_lossy(&serviced_remote.stdout);
    assert!(output.contains(&fs::read_to_string("scripts/Caddyfile.host").unwrap()));
    assert!(output.contains("REMOTE_DOMAIN=risak.dev REMOTE_IP=100.67.154.69"));
    assert!(!devvm_home
        .join("root/.config/devvm/Caddyfile.host")
        .exists());

    // 3. Remote service setup with overrides
    fs::write(&command_log, "").unwrap();
    let serviced_override = Command::new(&setup)
        .args([
            "--skip-image",
            "--service",
            "--remote",
            "--remote-domain",
            "custom.org",
            "--remote-ip",
            "100.64.0.1",
        ])
        .current_dir(&devvm_home)
        .env("HOME", &home)
        .env("DEVVM_HOME", &devvm_home)
        .env("PATH", &path)
        .env("FAKE_SMOLVM_INSTALL_LOG", &install_log)
        .env("FAKE_DAEMON_BUILD", &daemon_script)
        .env("FAKE_SERVICE_COMMAND_LOG", &command_log)
        .output()
        .unwrap();
    assert!(
        serviced_override.status.success(),
        "remote override setup failed: {serviced_override:?}"
    );

    let commands_override = fs::read_to_string(&command_log).unwrap();
    assert!(
        commands_override.contains("daemon service install --enable --remote-domain custom.org")
    );
    let output = String::from_utf8_lossy(&serviced_override.stdout);
    assert!(output.contains(
        "REMOTE_DOMAIN=custom.org REMOTE_IP=100.64.0.1 REMOTE_DOMAIN_REGEXP=custom\\.org"
    ));
    assert!(!devvm_home
        .join("root/.config/devvm/Caddyfile.host")
        .exists());

    // 4. Remote setup without --service prints config without touching system service
    fs::write(&command_log, "").unwrap();
    let setup_remote_only = Command::new(&setup)
        .args(["--skip-image", "--remote"])
        .current_dir(&devvm_home)
        .env("HOME", &home)
        .env("DEVVM_HOME", &devvm_home)
        .env("PATH", &path)
        .env("FAKE_SMOLVM_INSTALL_LOG", &install_log)
        .env("FAKE_DAEMON_BUILD", &daemon_script)
        .env("FAKE_SERVICE_COMMAND_LOG", &command_log)
        .output()
        .unwrap();
    assert!(setup_remote_only.status.success());
    assert!(!devvm_home
        .join("root/.config/devvm/Caddyfile.host")
        .exists());
    assert!(String::from_utf8_lossy(&setup_remote_only.stdout)
        .contains(&fs::read_to_string("scripts/Caddyfile.host").unwrap()));
    let commands_remote_only = fs::read_to_string(&command_log).unwrap();
    assert!(
        !commands_remote_only.contains("daemon service install"),
        "must not install service without --service"
    );
}

#[test]
fn test_devvm_project_name_truncation() {
    let temp = tempdir().unwrap();
    let devvm_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("devvm");

    let long_name = "extremely-long-project-directory-name-that-exceeds-forty-eight-chars-by-far";
    let project_dir = temp.path().join(long_name);
    fs::create_dir_all(&project_dir).unwrap();

    let output = Command::new("bash")
        .args([
            "-c",
            "source \"$1\" name >/dev/null; printf '%s' \"$PROJECT_HOST\"",
            "test",
        ])
        .arg(&devvm_bin)
        .current_dir(&project_dir)
        .output()
        .expect("Failed to compute devvm project host");
    assert!(output.status.success());
    let host = String::from_utf8(output.stdout).unwrap();
    assert_eq!(host.len(), 8);
    assert_eq!(
        host,
        devvm_daemon::models::compute_project_host(&project_dir)
    );
}
