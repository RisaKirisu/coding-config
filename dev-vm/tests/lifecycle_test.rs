//! Real HTTP disconnect and subprocess checks; no VM commands run on the host.
use devvm_daemon::{
    lifecycle::{command_output, LifecycleError, Operations},
    DaemonConfig,
};
use std::{path::Path, sync::Arc};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

async fn wait_file(path: &Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !path.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("file never appeared: {}", path.display()));
}

fn command_in(path: &Path, script: &str) -> Command {
    let mut command = Command::new("bash");
    command.args(["-c", script]).current_dir(path);
    command
}

fn config_in(path: &Path) -> DaemonConfig {
    let mut config = DaemonConfig::new();
    config.log_dir = path.join("logs");
    config
}

// A real shell and child, with a deliberately gated SIGTERM handler. This makes
// cleanup ordering observable without substituting a fake VM or guessing delays.
const GATED_COMMAND: &str = r#"
trap 'wait; printf terminated > terminated; while [ ! -f release ]; do sleep 0.01; done; exit 0' TERM
sleep 300 &
printf '%s %s' "$$" "$!" > ready
wait
"#;

struct ProcessFiles(tempfile::TempDir);
impl ProcessFiles {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn assert_reaped(&self) {
        let pids = std::fs::read_to_string(self.path().join("ready")).unwrap();
        for pid in pids.split_whitespace().map(|s| s.parse::<i32>().unwrap()) {
            assert_eq!(
                unsafe { libc::kill(pid, 0) },
                -1,
                "PID {pid} remains after cleanup"
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        }
    }
}
impl Drop for ProcessFiles {
    fn drop(&mut self) {
        // Only the command recorded in this test's private directory is targeted.
        if let Ok(pids) = std::fs::read_to_string(self.path().join("ready")) {
            if let Some(pid) = pids
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<i32>().ok())
            {
                if pid > 1 && pid != unsafe { libc::getpgrp() } {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn cancellation_waits_for_real_process_tree_before_releasing_project() {
    let files = ProcessFiles::new();
    let config = config_in(files.path());
    let operations = Operations::default();
    let project = Uuid::new_v4();
    let command = command_in(files.path(), GATED_COMMAND);
    let (ops, cfg) = (operations.clone(), config.clone());
    let request = tokio::spawn(async move {
        ops.run(&cfg, project, false, "VM start", move |cancel| async move {
            command_output(command, &cancel)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    });
    wait_file(&files.path().join("ready")).await;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    wait_file(&files.path().join("terminated")).await;
    assert!(matches!(
        operations
            .run(&config, project, false, "duplicate", |_| async { Ok(()) })
            .await,
        Err(LifecycleError::Busy)
    ));
    // A different Project is not blocked by this command's cleanup.
    operations
        .run(&config, Uuid::new_v4(), false, "other", |_| async {
            Ok(())
        })
        .await
        .unwrap();
    std::fs::write(files.path().join("release"), "").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match operations
                .run(&config, project, false, "retry", |_| async { Ok(()) })
                .await
            {
                Ok(()) => break,
                Err(LifecycleError::Busy) => tokio::task::yield_now().await,
                other => panic!("unexpected retry result: {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    files.assert_reaped();
    let log = std::fs::read_to_string(config.log_dir.join(project.to_string()).join("daemon.log"))
        .unwrap();
    assert!(log.contains("VM start cancelled; command cleanup finished"));
}

#[tokio::test]
async fn stop_reserves_project_and_waits_for_launch_cleanup() {
    let files = ProcessFiles::new();
    let config = config_in(files.path());
    let operations = Operations::default();
    let project = Uuid::new_v4();
    let command = command_in(files.path(), GATED_COMMAND);
    let (ops, cfg) = (operations.clone(), config.clone());
    let launch = tokio::spawn(async move {
        ops.run(
            &cfg,
            project,
            false,
            "DSH launch",
            move |cancel| async move {
                command_output(command, &cancel)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
        )
        .await
    });
    wait_file(&files.path().join("ready")).await;
    let stop_command = command_in(
        files.path(),
        "test -f terminated && test -f release && printf stopped > stopped",
    );
    let (ops, cfg) = (operations.clone(), config.clone());
    let stop = tokio::spawn(async move {
        ops.run(&cfg, project, true, "VM stop", move |cancel| async move {
            let output = command_output(stop_command, &cancel)
                .await
                .map_err(|e| e.to_string())?;
            assert!(output.status.success());
            Ok(())
        })
        .await
    });
    wait_file(&files.path().join("terminated")).await;
    assert!(!files.path().join("stopped").exists());
    assert!(matches!(
        operations
            .run(&config, project, false, "duplicate start", |_| async {
                Ok(())
            })
            .await,
        Err(LifecycleError::Busy)
    ));
    assert!(matches!(
        operations
            .run(&config, project, true, "duplicate stop", |_| async {
                Ok(())
            })
            .await,
        Err(LifecycleError::Busy)
    ));
    std::fs::write(files.path().join("release"), "").unwrap();
    assert!(matches!(
        launch.await.unwrap(),
        Err(LifecycleError::Cancelled)
    ));
    stop.await.unwrap().unwrap();
    assert!(files.path().join("stopped").exists());
    files.assert_reaped();
    operations
        .run(&config, project, false, "next start", |_| async { Ok(()) })
        .await
        .unwrap();
}

#[tokio::test]
async fn http_disconnect_interrupts_real_command_and_releases_ownership() {
    let files = Arc::new(ProcessFiles::new());
    // This command has a graceful handler but needs no externally released gate.
    std::fs::write(files.path().join("release"), "").unwrap();
    let config = config_in(files.path());
    let operations = Operations::default();
    let project = Uuid::new_v4();
    let (ops, cfg, process_files) = (operations.clone(), config.clone(), files.clone());
    let app = Router::new().route(
        "/start",
        post(move || {
            let (ops, cfg, files) = (ops.clone(), cfg.clone(), process_files.clone());
            async move {
                let command = command_in(files.path(), GATED_COMMAND);
                ops.run(&cfg, project, false, "VM start", move |cancel| async move {
                    command_output(command, &cancel)
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
                .await
                .unwrap();
                "finished"
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut socket = TcpStream::connect(addr).await.unwrap();
    socket
        .write_all(b"POST /start HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
        .await
        .unwrap();
    wait_file(&files.path().join("ready")).await;
    drop(socket);
    wait_file(&files.path().join("terminated")).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match operations
                .run(&config, project, false, "retry", |_| async { Ok(()) })
                .await
            {
                Ok(()) => break,
                Err(LifecycleError::Busy) => tokio::task::yield_now().await,
                other => panic!("unexpected result: {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    files.assert_reaped();
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn completed_failed_and_precancelled_commands_leave_no_operation() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_in(dir.path());
    let operations = Operations::default();
    let project = Uuid::new_v4();
    for stopping in [false, true] {
        for status in [0, 7] {
            let command = command_in(dir.path(), &format!("exit {status}"));
            let result = operations
                .run(
                    &config,
                    project,
                    stopping,
                    "command",
                    move |cancel| async move {
                        let output = command_output(command, &cancel)
                            .await
                            .map_err(|e| e.to_string())?;
                        if output.status.success() {
                            Ok(())
                        } else {
                            Err("command failed".into())
                        }
                    },
                )
                .await;
            assert_eq!(result.is_ok(), status == 0);
            operations
                .run(&config, project, false, "retry", |_| async { Ok(()) })
                .await
                .unwrap();
        }
    }
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = command_output(command_in(dir.path(), "touch must-not-exist"), &cancel).await;
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
    assert!(!dir.path().join("must-not-exist").exists());
}

use axum::{routing::post, Router};
use std::time::Duration;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

#[tokio::test]
async fn http_disconnect_drops_the_pending_handler() {
    let (entered_tx, entered_rx) = oneshot::channel();
    let (dropped_tx, dropped_rx) = oneshot::channel::<()>();
    let signals = std::sync::Arc::new(std::sync::Mutex::new(Some((entered_tx, dropped_tx))));
    let app = Router::new().route(
        "/start",
        post(move || {
            let (entered, dropped) = signals.lock().unwrap().take().unwrap();
            async move {
                let _drop_signal = dropped;
                entered.send(()).unwrap();
                std::future::pending::<()>().await;
                "finished"
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut socket = TcpStream::connect(addr).await.unwrap();
    socket
        .write_all(b"POST /start HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), entered_rx)
        .await
        .unwrap()
        .unwrap();
    drop(socket);
    let result = tokio::time::timeout(Duration::from_secs(3), dropped_rx).await;
    server.abort();
    assert!(
        matches!(result, Ok(Err(_))),
        "disconnect must drop the handler, got {result:?}"
    );
}
