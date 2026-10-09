//! Real RC2 browser fixture over the existing isolated DevVM platform seam.
//! Start explicitly with DEVVM_BROWSER_FIXTURE_ROOT and --ignored --nocapture.
mod common;

use common::{create_mock_devvm, CaddyGuard};
use devvm_daemon::{create_router, AppState, DaemonConfig, DshRuntimeManager, SyncManager};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command, time::Duration};
use tokio::net::TcpListener;

fn reserve_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test]
#[ignore = "owns real staged DSH children for the Playwright interaction suite"]
async fn serve_project_app_fixture() {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(18 * 60);
    let root = PathBuf::from(std::env::var("DEVVM_BROWSER_FIXTURE_ROOT").unwrap());
    assert!(root.is_absolute());
    assert!(root.starts_with(std::env::current_dir().unwrap().join(".agents")));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(root.join("tmp")).unwrap();
    let stage = root.parent().unwrap().join("stage");
    let remote = std::env::var("DEVVM_BROWSER_REMOTE").as_deref() == Ok("1");
    assert!(stage.join("cli/lib/bin.js").is_file());
    let bin_dir = root.join("bin");
    let logs = root.join("logs");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&logs).unwrap();
    let devvm = bin_dir.join("devvm");
    create_mock_devvm(&devvm, &logs);
    let launcher = bin_dir.join("mock_guest_bin/dsh");
    let launcher_text = format!(
        r#"#!/usr/bin/env bash
set -eu
port=$(cat "$PWD/.browser-port")
profile_args=(embedded-browser)
if [[ ! -f "$PWD/.dsh-home/profiles/embedded-browser/package.json" ]]; then
  profile_args+=(--from-default-profile web)
fi
exec env -i PATH=/usr/local/bin:/usr/bin:/bin HOME="$PWD/.os-home" \
  DSH_HOME="$PWD/.dsh-home" TMPDIR="{}/.agents/tmp" \
  XDG_CONFIG_HOME="$PWD/.os-home/.config" XDG_CACHE_HOME="$PWD/.os-home/.cache" \
  DEVVM_EMBED_PROJECT_ID="${{DEVVM_EMBED_PROJECT_ID:?}}" \
  DEVVM_CONTROL_ORIGINS="${{DEVVM_CONTROL_ORIGINS:?}}" \
  DEVVM_CONTROL_DOMAIN="${{DEVVM_CONTROL_DOMAIN:-}}" \
  node "{}/cli/lib/bin.js" "${{profile_args[@]}}" \
  --patch "{}/browser.overlay.yml" --host 127.0.0.1 --port "$port" --no-open
"#,
        std::env::current_dir().unwrap().display(),
        stage.display(),
        stage.display()
    );
    fs::write(&launcher, launcher_text).unwrap();
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755)).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let ingress = reserve_port();
    let home = root.join("projects");
    fs::create_dir_all(&home).unwrap();
    let config = DaemonConfig {
        host: "127.0.0.1".into(),
        port,
        config_path: root.join("registry.json"),
        sync_config_path: root.join("sync.json"),
        log_dir: logs,
        home_dir: home.clone(),
        devvm_bin: devvm,
        ingress_port: ingress,
        remote_domain: remote.then(|| "devvm.test".into()),
    };
    let router = create_router(AppState {
        config,
        dsh_runtime_manager: DshRuntimeManager::new(),
        sync_manager: SyncManager::new(),
    });
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let api = format!("http://127.0.0.1:{port}");
    let mut records = Vec::new();
    let mut routes = Vec::new();
    for name in ["Project A", "Project B"] {
        let path = home.join(name.replace(' ', "-"));
        fs::create_dir_all(path.join(".os-home")).unwrap();
        let dsh_port = reserve_port();
        fs::write(path.join(".browser-port"), dsh_port.to_string()).unwrap();
        let response = client
            .post(format!("{api}/api/projects/register"))
            .json(&json!({"path":path}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let project: Value = response.json().await.unwrap();
        let id = project["id"].as_str().unwrap();
        let host = project["project_host"].as_str().unwrap();
        routes.push(format!(
            r#"@project_{dsh_port} host 3080.{host}.devvm.localhost settings.{host}.devvm.test
reverse_proxy @project_{dsh_port} 127.0.0.1:{dsh_port} {{
  header_up Host localhost:{dsh_port}
  header_up Origin ^http://(3080\.{host}\.devvm\.localhost|settings\.{host}\.devvm\.test)(:[0-9]+)?$ http://localhost:{dsh_port}
}}"#
        ));
        records.push(json!({"id":id,"name":name,"host":host,"dshPort":dsh_port,"path":path}));
    }
    let caddy_config = root.join("Caddyfile");
    let auto_https = if remote { "disable_redirects" } else { "off" };
    let mut caddy_text = format!("{{\n admin off\n auto_https {auto_https}\n skip_install_trust\n}}\nhttp://:{ingress} {{\n route {{\n{}\n respond 400\n }}\n}}\n", routes.join("\n"));
    if remote {
        let remote_routes = include_str!("../scripts/Caddyfile.host")
            .replace("{$REMOTE_DOMAIN:risak.dev}", "devvm.test")
            .replace("{$REMOTE_DOMAIN_REGEXP:risak\\.dev}", "devvm\\.test")
            .replace("{$REMOTE_IP:100.67.154.69}", "127.0.0.2")
            .replace(
                "tls {\n\t\tdns cloudflare {env.CLOUDFLARE_API_TOKEN}\n\t}",
                "tls internal",
            )
            .replace("127.0.0.1:8100", &format!("127.0.0.1:{port}"))
            .replace(":8102", &format!(":{ingress}"));
        caddy_text.push_str(&remote_routes);
    }
    fs::write(&caddy_config, caddy_text).unwrap();
    let caddy_log = fs::File::create(root.join("caddy.log")).unwrap();
    let _caddy = CaddyGuard(Some(
        Command::new("/usr/local/bin/caddy")
            .args(["run", "--config"])
            .arg(&caddy_config)
            .arg("--adapter")
            .arg("caddyfile")
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", root.join("caddy-home"))
            .env("XDG_DATA_HOME", root.join("caddy-data"))
            .env("XDG_CONFIG_HOME", root.join("caddy-config"))
            .stdout(caddy_log.try_clone().unwrap())
            .stderr(caddy_log)
            .spawn()
            .unwrap(),
    ));
    fs::write(
        root.join("ready.json"),
        serde_json::to_vec_pretty(&json!({
            "controlUrl":format!("http://control.devvm.localhost:{port}"),
            "apiUrl":api,"ingressPort":ingress,"projects":records,
            "remoteControlUrls": remote.then(|| vec!["https://devvm.devvm.test", "https://devvm-risak.devvm.test", "https://devvm--home.devvm.test", "https://devvm-3080.devvm.test"])
        }))
        .unwrap(),
    )
    .unwrap();
    println!("Real daemon and isolated Caddy ready; no Project Runtime is started automatically.");
    let mut interval = tokio::time::interval(Duration::from_millis(200));
    while !root.join("stop").exists() && tokio::time::Instant::now() < deadline {
        interval.tick().await;
    }
    for record in &records {
        let id = record["id"].as_str().unwrap();
        client
            .post(format!("{api}/api/projects/{id}/dsh/stop"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }
    server.abort();
}
