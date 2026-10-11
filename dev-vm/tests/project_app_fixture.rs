//! Real staged DSH processes and an isolated daemon/Caddy browser carrier.
//! Start explicitly with DEVVM_BROWSER_FIXTURE_ROOT and --ignored --nocapture.
mod common;

use common::CaddyGuard;
use devvm_daemon::models::{compute_project_host, ProjectView};
use devvm_daemon::runner::ProcessCommandRunner;
use devvm_daemon::{create_router, AppState, DaemonConfig};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Arc,
    time::Duration,
};
use tokio::net::TcpListener;
use uuid::Uuid;

fn reserve_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct DshChild {
    child: Option<Child>,
    stage: PathBuf,
    path: PathBuf,
    id: Uuid,
    port: u16,
    origins: String,
    remote: bool,
    log: PathBuf,
}

impl DshChild {
    fn start(stage: &Path, path: &Path, id: Uuid, port: u16, origins: &str, remote: bool) -> Self {
        let log = path.join("dsh-startup.log");
        let mut owner = Self {
            child: None,
            stage: stage.to_owned(),
            path: path.to_owned(),
            id,
            port,
            origins: origins.to_owned(),
            remote,
            log,
        };
        owner.launch();
        owner
    }

    fn launch(&mut self) {
        let mut command = Command::new("node");
        command
            .current_dir(&self.path)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", self.path.join(".os-home"))
            .env("DSH_HOME", self.path.join(".dsh-home"))
            .env(
                "TMPDIR",
                std::env::current_dir().unwrap().join(".agents/tmp"),
            )
            .env("XDG_CONFIG_HOME", self.path.join(".os-home/.config"))
            .env("XDG_CACHE_HOME", self.path.join(".os-home/.cache"))
            .env("DEVVM_EMBED_PROJECT_ID", self.id.to_string())
            .env("DEVVM_CONTROL_ORIGINS", &self.origins)
            .env(
                "DEVVM_CONTROL_DOMAIN",
                if self.remote { "devvm.test" } else { "" },
            )
            .arg(self.stage.join("cli/lib/bin.js"))
            .arg("embedded-browser");
        if !self
            .path
            .join(".dsh-home/profiles/embedded-browser/package.json")
            .exists()
        {
            command.args(["--from-default-profile", "web"]);
        }
        command
            .arg("--patch")
            .arg(self.stage.join("browser.overlay.yml"))
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &self.port.to_string(),
                "--no-open",
            ])
            .process_group(0);
        let output = fs::File::create(&self.log).unwrap();
        command.stdout(output.try_clone().unwrap()).stderr(output);
        self.child = Some(command.spawn().unwrap());
    }

    async fn launch_token(&mut self) -> String {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        loop {
            let text = fs::read_to_string(&self.log).unwrap();
            if let Some(line) = text
                .lines()
                .find(|line| line.contains("dsh web:") && line.contains("token="))
            {
                let url = line
                    .split_whitespace()
                    .find(|word| word.contains("token="))
                    .unwrap();
                return reqwest::Url::parse(url)
                    .unwrap()
                    .query_pairs()
                    .find(|(key, _)| key == "token")
                    .unwrap()
                    .1
                    .into_owned();
            }
            assert!(
                self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                "DSH exited; see {}",
                self.log.display()
            );
            assert!(
                tokio::time::Instant::now() < deadline,
                "DSH did not start; see {}",
                self.log.display()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGTERM);
            }
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            while child.try_wait().unwrap().is_none() && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            child.wait().unwrap();
        }
    }
}

impl Drop for DshChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            child.wait().unwrap();
        }
    }
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
    let logs = root.join("logs");
    fs::create_dir_all(&logs).unwrap();
    let devvm = root.join("unavailable-devvm");
    assert!(!devvm.exists());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let ingress = reserve_port();
    let https_port = std::net::TcpListener::bind("127.0.0.2:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let remote_origins: Vec<String> = ["devvm", "devvm-risak", "devvm--home", "devvm-3080"]
        .iter()
        .map(|label| format!("https://{label}.devvm.test"))
        .collect();
    let control_url = format!("http://control.devvm.localhost:{port}");
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
    let router = create_router(AppState::new(config, Arc::new(ProcessCommandRunner)));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let api = format!("http://127.0.0.1:{port}");
    let daemon_instance = Uuid::new_v4();
    let mut children = Vec::new();
    let mut records = Vec::new();
    let mut routes = Vec::new();
    for name in ["Project A", "Project B"] {
        let path = home.join(name.replace(' ', "-"));
        fs::create_dir_all(path.join(".os-home")).unwrap();
        let dsh_port = reserve_port();
        let id = devvm_daemon::registry::register_project(
            &root.join("registry.json"),
            path.to_str().unwrap(),
        )
        .unwrap()
        .id;
        let host = compute_project_host(&path);
        let mut origins = vec![control_url.clone()];
        if remote {
            origins.extend(remote_origins.clone());
        }
        let mut child = DshChild::start(
            &stage,
            &path,
            id,
            dsh_port,
            &serde_json::to_string(&origins).unwrap(),
            remote,
        );
        let token = child.launch_token().await;
        let local_url = format!("http://3080.{host}.devvm.localhost:{ingress}/?token={token}");
        let tailnet_url = remote.then(|| format!("https://{host}-3080.devvm.test/?token={token}"));
        let view = json!({
            "id":id,"path":path,"name":path.file_name().unwrap().to_str().unwrap(),"project_host":host,
            "vm_status":"running","dsh_status":"running","sync_status":"not_configured",
            "links":{"local_dsh_url":local_url,"tailnet_dsh_url":tailnet_url,
                "local_port_template":format!("http://{{port}}.{host}.devvm.localhost:{ingress}"),
                "tailnet_port_template":remote.then(|| format!("https://{host}-{{port}}.devvm.test"))},
            "daemon_instance_id":daemon_instance,"operation":null,"last_operation":null
        });
        let view: ProjectView = serde_json::from_value(view).unwrap();
        children.push(child);
        routes.push(format!(
            r#"@project_{dsh_port} host 3080.{host}.devvm.localhost settings.{host}.devvm.test
reverse_proxy @project_{dsh_port} 127.0.0.1:{dsh_port} {{
  header_up Host localhost:{dsh_port}
  header_up Origin ^http://(3080\.{host}\.devvm\.localhost|settings\.{host}\.devvm\.test)(:[0-9]+)?$ http://localhost:{dsh_port}
}}"#
        ));
        records.push(json!({"id":id,"name":name,"host":host,"dshPort":dsh_port,"path":path,"token":token,"view":view}));
    }
    let caddy_config = root.join("Caddyfile");
    let auto_https = if remote { "disable_redirects" } else { "off" };
    let mut caddy_text = format!("{{\n admin off\n auto_https {auto_https}\n skip_install_trust\n}}\nhttp://:{ingress} {{\n route {{\n{}\n respond 400\n }}\n}}\n", routes.join("\n"));
    if remote {
        let remote_routes = include_str!("../scripts/Caddyfile.host")
            .replace(
                "{$REMOTE_DOMAIN:risak.dev}",
                &format!("devvm.test:{https_port}"),
            )
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
            "controlUrl":control_url,
            "apiUrl":api,"ingressPort":ingress,"remoteTransportPort":https_port,"projects":records,
            "remoteControlUrls": remote.then_some(remote_origins)
        }))
        .unwrap(),
    )
    .unwrap();
    println!("Real daemon, isolated Caddy and staged Project DSH processes ready.");
    let mut last_request = String::new();
    let mut interval = tokio::time::interval(Duration::from_millis(200));
    while !root.join("stop").exists() && tokio::time::Instant::now() < deadline {
        interval.tick().await;
        let request_file = root.join("process-request.json");
        if request_file.exists() {
            let request: Value = serde_json::from_slice(&fs::read(&request_file).unwrap()).unwrap();
            let request_id = request["request_id"].as_str().unwrap();
            Uuid::parse_str(request_id).unwrap();
            if request_id == last_request {
                continue;
            }
            last_request = request_id.to_owned();
            let index = records
                .iter()
                .position(|record| record["id"] == request["id"])
                .unwrap();
            let action = request["action"].as_str().unwrap();
            assert!(["stop", "start", "restart"].contains(&action));
            if action != "start" {
                children[index].stop().await;
            }
            if action != "stop" {
                assert!(children[index].child.is_none());
                children[index].launch();
                let token = children[index].launch_token().await;
                records[index]["token"] = json!(token);
                for key in ["local_dsh_url", "tailnet_dsh_url"] {
                    if let Some(url) = records[index]["view"]["links"][key].as_str() {
                        let mut url = reqwest::Url::parse(url).unwrap();
                        url.query_pairs_mut().clear().append_pair("token", &token);
                        records[index]["view"]["links"][key] = json!(url.as_str());
                    }
                }
            }
            records[index]["view"]["dsh_status"] = json!(if action == "stop" {
                "stopped"
            } else {
                "running"
            });
            let mut ready: Value =
                serde_json::from_slice(&fs::read(root.join("ready.json")).unwrap()).unwrap();
            ready["projects"] = json!(records);
            fs::write(
                root.join("ready.next.json"),
                serde_json::to_vec_pretty(&ready).unwrap(),
            )
            .unwrap();
            fs::rename(root.join("ready.next.json"), root.join("ready.json")).unwrap();
            fs::write(
                root.join(format!("process-result-{request_id}.json")),
                serde_json::to_vec(&ready).unwrap(),
            )
            .unwrap();
        }
    }
    for child in &mut children {
        child.stop().await;
    }
    server.abort();
    let _ = server.await;
}
