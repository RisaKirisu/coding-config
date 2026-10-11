use devvm_daemon::{
    generate_launchd_plist, generate_systemd_unit, get_launchd_plist_path,
    get_systemd_service_path, Platform, ServiceManager, ServicePlistConfig, ServiceUnitConfig,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_web_profile_installs_local_plugins_as_packages() {
    let profile_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("root/.dsh/profiles/web/package.json");
    let profile: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&profile_path).unwrap()).unwrap();
    let dependencies = profile["dependencies"].as_object().unwrap();

    for (package, source) in [
        ("@devvm/dsh-agent-presets", "agent-presets"),
        ("@devvm/dsh-build-loop", "build-loop"),
        ("@devvm/dsh-remote-sync", "remote-sync"),
        ("@devvm/dsh-style-control", "style-control"),
        ("@devvm/dsh-subagent-manager", "subagent-manager"),
        ("@devvm/dsh-voice-input", "voice-input"),
        ("dsh-skill-mcp-panel", "dsh-skill-mcp-panel"),
    ] {
        assert_eq!(
            dependencies.get(package).and_then(|value| value.as_str()),
            Some(format!("file:../../plugins/{source}").as_str()),
            "{package} must use a portable native package dependency"
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
        Some("0.3.6")
    );
    assert_eq!(
        deps.get("dsh-better-sidebar").and_then(|v| v.as_str()),
        Some("0.24.1")
    );
}

#[test]
fn test_web_profile_lockfile_packages_local_plugins() {
    let lockfile_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("root/.dsh/profiles/web/pnpm-lock.yaml");
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    for plugin in [
        "agent-presets",
        "build-loop",
        "remote-sync",
        "style-control",
        "subagent-manager",
        "voice-input",
        "dsh-skill-mcp-panel",
    ] {
        assert!(
            lockfile_content.contains(&format!("specifier: file:../../plugins/{plugin}")),
            "lockfile must install {plugin} as a local package"
        );
        assert!(
            lockfile_content.contains(&format!(
                "resolution: {{directory: ../../plugins/{plugin}, type: directory}}"
            )),
            "lockfile must resolve {plugin} through pnpm's package installer"
        );
        assert!(
            !lockfile_content.contains(&format!("link:/root/.dsh/plugins/{plugin}")),
            "lockfile must not link directly to {plugin} source"
        );
    }

    assert!(
        lockfile_content.contains("'@hytime/dsh-thinking-effort@0.3.6':"),
        "lockfile must contain @hytime/dsh-thinking-effort locked at 0.3.6"
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

// Real Bash sources the production functions without running setup or a VM.
fn script_function(script: &str, body: &str, args: &[&str], stdin: &str) -> Vec<String> {
    use std::io::Write;
    use std::process::Stdio;

    let temp = tempdir().unwrap();
    let mut child = Command::new("bash")
        .args(["-c", &format!("source \"$1\"; shift; {body}"), "test"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(script))
        .args(args)
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env("DEVVM_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{script}: {body}: {output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    if stdout.contains('\0') {
        stdout.split_terminator('\0').map(str::to_owned).collect()
    } else {
        stdout.lines().map(str::to_owned).collect()
    }
}

#[test]
fn test_scripts_can_be_sourced_without_actions() {
    for script in ["devvm", "setup-devvm.sh"] {
        assert_eq!(
            script_function(
                script,
                "printf '%s\\n' sourced; test -z \"$(ls -A)\"",
                &["start"],
                ""
            ),
            ["sourced"]
        );
    }
}

#[test]
fn test_devvm_smolvm_command_arguments() {
    // smolvm v1.19.3: README.md, docs/smolfile.md and src/cli/machine.rs.
    // https://github.com/smol-machines/smolvm/tree/v1.19.3
    let context = r#"NAME=dev-project-abcd1234; SMOLFILE='/vm home/smolvm.toml'; IMAGE='/vm home/image.tar'; PROJECT_HOST=project-abcd1234; PROJECT_DIR='/project dir'; DEVVM_ROOT='/vm home/root'; "#;
    assert_eq!(
        script_function("devvm", &format!("{context}smolvm_create_args"), &[], ""),
        [
            "machine",
            "create",
            "--name",
            "dev-project-abcd1234",
            "-s",
            "/vm home/smolvm.toml",
            "--net-backend",
            "virtio-net",
            "--image",
            "/vm home/image.tar",
            "-e",
            "DEVVM_PROJECT_HOST=project-abcd1234",
            "-v",
            "/project dir:/root/workspace",
            "-v",
            "/vm home/root:/devvm-root",
        ]
    );
    for (function, expected) in [
        (
            "smolvm_start_args",
            vec!["machine", "start", "--name", "dev-project-abcd1234"],
        ),
        (
            "smolvm_stop_args",
            vec!["machine", "stop", "--name", "dev-project-abcd1234"],
        ),
        (
            "smolvm_delete_args",
            vec!["machine", "delete", "--name", "dev-project-abcd1234", "-f"],
        ),
        ("smolvm_list_args", vec!["machine", "ls", "--json"]),
    ] {
        assert_eq!(
            script_function("devvm", &format!("{context}{function}"), &[], ""),
            expected
        );
    }
    for (mode, flag) in [
        ("plain", None),
        ("stream", Some("--stream")),
        ("interactive", Some("-it")),
    ] {
        let mut expected = vec!["machine", "exec"];
        expected.extend(flag);
        expected.extend([
            "--name",
            "dev-project-abcd1234",
            "--",
            "/bin/sh",
            "-c",
            "printf '%s' \"$1\"",
            "guest",
            "a b",
            "",
        ]);
        assert_eq!(
            script_function(
                "devvm",
                &format!("{context}smolvm_exec_args \"$@\""),
                &[
                    mode,
                    "/bin/sh",
                    "-c",
                    "printf '%s' \"$1\"",
                    "guest",
                    "a b",
                    ""
                ],
                ""
            ),
            expected
        );
    }
}

#[test]
fn test_devvm_ingress_and_guest_initialization_arguments() {
    assert_eq!(script_function("devvm",
        "FRPS_BIN='/bin dir/frps'; INGRESS_PORT=17007; HTTP_PORT=8102; FRPS_TIMEOUT=1800; ingress_args", &[], ""),
        ["/bin dir/frps", "--bind-addr", "127.0.0.1", "--bind-port", "17007", "--proxy-bind-addr", "127.0.0.1", "--vhost-http-port", "8102", "--vhost-http-timeout", "1800"]);
    assert_eq!(
        script_function("devvm", "NAME=project; start_ingress_args", &[], ""),
        [
            "machine",
            "exec",
            "--name",
            "project",
            "--",
            "/usr/local/bin/devvm-ingress"
        ]
    );
    assert_eq!(
        script_function("devvm", "NAME=project; clear_crane_args", &[], ""),
        [
            "machine",
            "exec",
            "--name",
            "project",
            "--",
            "/bin/bash",
            "-c",
            "rm -rf /storage/tmp/* && fstrim /storage"
        ]
    );
    for (builder, marker, required) in [
        ("link_root_args", "link-root", "CI=true DSH_HOME=/root/.dsh dsh plugin --profile \"$name\" install --frozen-lockfile --store-dir \"$store\""),
        ("start_cache_reclaim_args", "cache-reclaim", "reclaim_cache_above_budget </dev/null >/dev/null 2>&1 &"),
    ] {
        let args = script_function("devvm", &format!("NAME=project; {builder}"), &[], "");
        assert_eq!(&args[..7], ["machine", "exec", "--name", "project", "--", "/bin/bash", "-c"]);
        assert_eq!(args.len(), 9);
        assert_eq!(args[8], marker);
        assert!(args[7].contains(required), "{}", args[7]);
        assert!(Command::new("bash").args(["-n", "-c", &args[7]]).status().unwrap().success());
        if builder == "start_cache_reclaim_args" {
            assert!(args[7].contains("local budget_bytes=$((512 * 1024 * 1024))"));
            assert!(args[7].contains("while sleep 30; do"));
            assert!(args[7].contains("/sys/fs/cgroup/memory.reclaim || true"));
        }
    }
}

#[test]
fn test_devvm_machine_state_decisions() {
    // v1.19.3 src/cli/vm_common.rs: machine_status_json/list_vms emits an array.
    for (json, exists, running) in [
        ("[]", "false", "false"),
        (
            r#"[{"name":"other","state":"running","memory_mib":8192,"pid":123}]"#,
            "false",
            "false",
        ),
        (
            r#"[{"name":"project","state":"stopped","memory_mib":8192,"pid":null}]"#,
            "true",
            "false",
        ),
        (
            r#"[{"name":"other","state":"stopped"},{"name":"project","state":"running","memory_mib":8192,"pid":123}]"#,
            "true",
            "true",
        ),
        (
            r#"[{"name":"project","state":"unreachable"}]"#,
            "true",
            "false",
        ),
    ] {
        assert_eq!(
            script_function("devvm", "machine_exists \"$1\"", &["project"], json),
            [exists]
        );
        assert_eq!(
            script_function("devvm", "machine_running \"$1\"", &["project"], json),
            [running]
        );
    }
}

#[test]
fn test_devvm_project_identity() {
    for (path, name) in [
        ("/work/My Project!", "my-project"),
        ("/work/---", "project"),
        ("/work/Project-", "project"),
    ] {
        assert_eq!(
            script_function("devvm", "project_name \"$1\"", &[path], ""),
            [name]
        );
    }
    assert_eq!(
        script_function("devvm", "project_hash \"$1\"", &["/work/project"], ""),
        ["65d80d2c"]
    );
    assert_eq!(
        script_function("devvm", "machine_name \"$@\"", &["project", "abcd1234"], ""),
        ["dev-project-abcd1234"]
    );
    for (length, expected) in [
        (48, format!("{}-abcd1234", "a".repeat(48))),
        (49, "abcd1234".into()),
    ] {
        assert_eq!(
            script_function(
                "devvm",
                "project_host \"$@\"",
                &[&"a".repeat(length), "abcd1234"],
                ""
            ),
            [expected]
        );
    }
}

#[test]
fn test_devvm_memory_json() {
    for (host, limit, meminfo, expected) in [
        (
            "123456",
            "8192",
            "MemTotal: 8388608 kB\nMemAvailable: 7340032 kB\n",
            r#"{"host_bytes":123456,"guest_bytes":1073741824,"limit_bytes":8589934592}"#,
        ),
        (
            "",
            "",
            "",
            r#"{"host_bytes":null,"guest_bytes":null,"limit_bytes":null}"#,
        ),
        (
            "0",
            "invalid",
            "MemTotal: 100 kB\n",
            r#"{"host_bytes":0,"guest_bytes":null,"limit_bytes":null}"#,
        ),
        (
            "",
            "0",
            "MemTotal: 100 kB\nMemAvailable: 101 kB\n",
            r#"{"host_bytes":null,"guest_bytes":null,"limit_bytes":0}"#,
        ),
        (
            "",
            "1",
            "MemTotal: 100 kB\nMemAvailable: 100 kB\n",
            r#"{"host_bytes":null,"guest_bytes":0,"limit_bytes":1048576}"#,
        ),
    ] {
        assert_eq!(
            script_function("devvm", "memory_json \"$@\"", &[host, limit], meminfo),
            [expected]
        );
    }
}

#[test]
fn test_setup_version_decisions_and_downloads() {
    for (installed, expected) in [
        ("", "install"),
        ("1.19.3", "skip"),
        ("1.12.0", "replace"),
        ("1.20.0", "replace"),
    ] {
        assert_eq!(
            script_function(
                "setup-devvm.sh",
                "smolvm_install_decision \"$1\"",
                &[installed],
                ""
            ),
            [expected]
        );
    }
    assert_eq!(
        script_function("setup-devvm.sh", "smolvm_installer_url", &[], ""),
        ["https://smolmachines.com/install.sh"]
    );
    assert_eq!(
        script_function("setup-devvm.sh", "smolvm_installer_args", &[], ""),
        ["-s", "--", "--version", "1.19.3"]
    );
    for (os, arch) in [
        ("linux", "amd64"),
        ("linux", "arm64"),
        ("darwin", "amd64"),
        ("darwin", "arm64"),
    ] {
        assert_eq!(script_function("setup-devvm.sh", "frp_download_url \"$@\"", &[os, arch], ""),
            [format!("https://github.com/fatedier/frp/releases/download/v0.71.0/frp_0.71.0_{os}_{arch}.tar.gz")]);
    }
    assert_eq!(
        script_function(
            "setup-devvm.sh",
            r#"mkdir -p "$HOME/.smolvm"; printf ' 1.19.3\n' > "$HOME/.smolvm/.version"; installed_smolvm_version"#,
            &[],
            ""
        ),
        ["1.19.3"]
    );
}

#[test]
fn test_setup_service_arguments_and_start_choice() {
    for (os, method) in [("Linux", "systemctl"), ("Darwin", "launchd")] {
        assert_eq!(
            script_function("setup-devvm.sh", "service_start_method \"$1\"", &[os], ""),
            [method]
        );
        for domain in ["", "risak.dev", "custom.org"] {
            let mut expected = vec!["service", "install", "--enable"];
            if !domain.is_empty() {
                expected.extend(["--remote-domain", domain]);
            }
            if os == "Darwin" {
                expected.push("--start");
            }
            assert_eq!(
                script_function(
                    "setup-devvm.sh",
                    "daemon_service_args \"$@\"",
                    &[os, domain],
                    ""
                ),
                expected
            );
        }
    }
    assert_eq!(
        script_function("setup-devvm.sh", "service_restart_args", &[], ""),
        ["--user", "restart", "devvm-daemon.service"]
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
            "source \"$1\"; dir=\"$(realpath \"$PWD\")\"; project_host \"$(project_name \"$dir\")\" \"$(project_hash \"$dir\")\"",
            "test",
        ])
        .arg(&devvm_bin)
        .current_dir(&project_dir)
        .output()
        .expect("Failed to compute devvm project host");
    assert!(output.status.success());
    let host = String::from_utf8(output.stdout)
        .unwrap()
        .trim_end()
        .to_string();
    assert_eq!(host.len(), 8);
    assert_eq!(
        host,
        devvm_daemon::models::compute_project_host(&project_dir)
    );
}
