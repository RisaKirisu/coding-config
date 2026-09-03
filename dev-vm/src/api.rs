use crate::browser::{browse_directory, BrowserError};
use crate::config::DaemonConfig;
use crate::logs::{append_log_logged, read_recent_logs};
use crate::models::{
    compute_project_host, ActionResponse, DshStatus, LogsResponse, OpenPortRequest,
    OpenPortResponse, ProjectLinks, ProjectRecord, ProjectView, RegisterRequest,
    SyncConfigResponse, SyncDeleteRequest, SyncSetupRequest,
};
use crate::registry::{
    get_project, load_projects, register_project, unregister_project, RegistryError,
};
use crate::runner::{check_vm_status, run_vm_delete, run_vm_start, run_vm_stop};
use crate::runtime::DshRuntimeManager;
use crate::sync::{load_sync_config, provision_sync_setup, SyncConfig, SyncManager};
use crate::ui::INDEX_HTML;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub config: DaemonConfig,
    pub dsh_runtime_manager: DshRuntimeManager,
    pub sync_manager: SyncManager,
}

/// The single place a handler error is recorded: one `tracing` event, one `[daemon:error]`
/// line in the Project's `daemon.log` when the Project is known, and the JSON error body.
/// Callers that fail must not append their own error line, or the viewer shows it twice.
fn api_error(
    status: StatusCode,
    log_dir: &std::path::Path,
    project: Option<Uuid>,
    context: &str,
    error: impl std::fmt::Display,
) -> Response {
    let error = error.to_string();
    tracing::error!(project = ?project, context, error = %error, "request failed");
    if let Some(project_id) = project {
        append_log_logged(log_dir, project_id, "daemon:error", &error);
    }
    (status, Json(json!({ "error": error }))).into_response()
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/api/projects", get(list_projects))
        .route("/api/projects/{id}", get(get_project_handler))
        .route("/api/projects/register", post(register_project_handler))
        .route(
            "/api/projects/{id}/unregister",
            post(unregister_project_handler),
        )
        .route("/api/projects/{id}/logs", get(get_logs_handler))
        .route("/api/projects/{id}/vm/start", post(start_vm_handler))
        .route("/api/projects/{id}/vm/stop", post(stop_vm_handler))
        .route("/api/projects/{id}/vm/delete", post(delete_vm_handler))
        .route("/api/projects/{id}/dsh/launch", post(launch_dsh_handler))
        .route("/api/projects/{id}/dsh/stop", post(stop_dsh_handler))
        .route("/api/projects/{id}/dsh/restart", post(restart_dsh_handler))
        .route("/api/projects/{id}/open-port", post(open_port_handler))
        .route("/api/projects/{id}/sync/delete", post(delete_sync_handler))
        .route("/api/sync/config", get(get_sync_config_handler))
        .route("/api/sync/setup", post(setup_sync_handler))
        .route("/api/browser", get(browser_handler))
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(
                    tower_http::trace::DefaultMakeSpan::new().level(tracing::Level::INFO),
                )
                .on_response(
                    tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO),
                ),
        )
        .with_state(Arc::new(state))
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn build_project_view(state: &AppState, record: &ProjectRecord) -> ProjectView {
    let name = record
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();

    let project_host = compute_project_host(&record.path);
    let vm_status = check_vm_status(&state.config, &record.path).await;
    let dsh_status = state
        .dsh_runtime_manager
        .get_status(&state.config, record.id, &record.path)
        .await;

    let is_configured = load_sync_config(&state.config.sync_config_path)
        .map(|c| c.is_some())
        .unwrap_or(false);
    // `devvm exec` auto-creates and starts a DevVM, so the runner must never be called
    // for a Project whose DevVM is not already running.
    let sync_status = if is_configured && vm_status == crate::models::VmStatus::Running {
        state.sync_manager.read_status(&record.path).await
    } else {
        None
    };

    let (local_dsh_url, tailnet_dsh_url) = if dsh_status == DshStatus::Running {
        (
            Some(format!(
                "http://3080.{}.devvm.localhost:{}",
                project_host, state.config.ingress_port
            )),
            Some(format!(
                "http://3080.{}.{}:{}",
                project_host, state.config.tailnet_domain, state.config.ingress_port
            )),
        )
    } else {
        (None, None)
    };

    let local_port_template = format!(
        "http://{{port}}.{}.devvm.localhost:{}",
        project_host, state.config.ingress_port
    );
    let tailnet_port_template = format!(
        "http://{{port}}.{}.{}:{}",
        project_host, state.config.tailnet_domain, state.config.ingress_port
    );

    ProjectView {
        id: record.id,
        path: record.path.display().to_string(),
        name,
        project_host,
        vm_status,
        dsh_status,
        sync_status,
        links: ProjectLinks {
            local_dsh_url: local_dsh_url.clone(),
            tailnet_dsh_url,
            dsh_url: local_dsh_url,
            local_port_template: local_port_template.clone(),
            tailnet_port_template,
            port_url_template: Some(local_port_template),
        },
    }
}

async fn list_projects(State(state): State<Arc<AppState>>) -> Response {
    let records = match load_projects(&state.config.config_path) {
        Ok(r) => r,
        Err(e) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &state.config.log_dir,
                None,
                "list_projects",
                format!("Failed to load projects: {}", e),
            );
        }
    };

    let mut views = Vec::new();
    for rec in &records {
        views.push(build_project_view(&state, rec).await);
    }

    Json(views).into_response()
}

fn get_project_or_response(state: &AppState, id: Uuid) -> Result<ProjectRecord, Box<Response>> {
    match get_project(&state.config.config_path, id) {
        Ok(Some(p)) => Ok(p),
        Ok(None) => Err(Box::new(api_error(
            StatusCode::NOT_FOUND,
            &state.config.log_dir,
            None,
            "get_project",
            "Project not found",
        ))),
        Err(e) => Err(Box::new(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            None,
            "get_project",
            format!("Registry error: {}", e),
        ))),
    }
}

async fn get_project_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };
    let view = build_project_view(&state, &project).await;
    Json(view).into_response()
}

async fn register_project_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterRequest>,
) -> Response {
    match register_project(&state.config.config_path, &payload.path) {
        Ok(rec) => {
            let view = build_project_view(&state, &rec).await;
            Json(view).into_response()
        }
        Err(RegistryError::PathNotFound(p)) => api_error(
            StatusCode::BAD_REQUEST,
            &state.config.log_dir,
            None,
            "register_project",
            format!("Path not found: {}", p),
        ),
        Err(RegistryError::NotADirectory(p)) => api_error(
            StatusCode::BAD_REQUEST,
            &state.config.log_dir,
            None,
            "register_project",
            format!("Path is not a directory: {}", p),
        ),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            None,
            "register_project",
            format!("Registration failed: {}", e),
        ),
    }
}

async fn unregister_project_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Response {
    match unregister_project(&state.config.config_path, id) {
        Ok(true) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("Project unregistered successfully".to_string()),
        })
        .into_response(),
        Ok(false) => api_error(
            StatusCode::NOT_FOUND,
            &state.config.log_dir,
            None,
            "unregister_project",
            "Project not found in registry",
        ),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            Some(id),
            "unregister_project",
            format!("Failed to unregister: {}", e),
        ),
    }
}

async fn get_logs_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let entries = read_recent_logs(&state.config.log_dir, id, 65536);
    Json(LogsResponse {
        project_id: id,
        entries,
    })
    .into_response()
}

async fn start_vm_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    match run_vm_start(&state.config, id, &project.path).await {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DevVM started".to_string()),
        })
        .into_response(),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            Some(id),
            "start_vm",
            e,
        ),
    }
}

async fn stop_vm_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    state
        .dsh_runtime_manager
        .handle_vm_stopped(&state.config, id, &project.path)
        .await;

    match run_vm_stop(&state.config, id, &project.path).await {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DevVM stopped".to_string()),
        })
        .into_response(),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            Some(id),
            "stop_vm",
            e,
        ),
    }
}

async fn delete_vm_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    state
        .dsh_runtime_manager
        .handle_vm_stopped(&state.config, id, &project.path)
        .await;

    match run_vm_delete(&state.config, id, &project.path).await {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DevVM deleted".to_string()),
        })
        .into_response(),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            Some(id),
            "delete_vm",
            e,
        ),
    }
}

async fn launch_dsh_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    match state
        .dsh_runtime_manager
        .launch_dsh(&state.config, id, &project.path)
        .await
    {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DSH launched".to_string()),
        })
        .into_response(),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            Some(id),
            "launch_dsh",
            e,
        ),
    }
}

async fn restart_dsh_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    // `stop_dsh` already reports an absent or stopped runtime as `Ok(())`.
    if let Err(e) = state
        .dsh_runtime_manager
        .stop_dsh(&state.config, id, &project.path)
        .await
    {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            Some(id),
            "restart_dsh",
            e,
        );
    }

    match state
        .dsh_runtime_manager
        .launch_dsh(&state.config, id, &project.path)
        .await
    {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DSH restarted".to_string()),
        })
        .into_response(),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            Some(id),
            "restart_dsh",
            e,
        ),
    }
}

async fn delete_sync_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<SyncDeleteRequest>,
) -> Response {
    if !payload.confirmed {
        return api_error(
            StatusCode::BAD_REQUEST,
            &state.config.log_dir,
            Some(id),
            "delete_sync",
            "Confirmation required: set confirmed=true to delete remote sync store",
        );
    }

    let _project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    match state
        .sync_manager
        .delete_sync_store(&state.config, id, true)
        .await
    {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("Remote sync store deleted".to_string()),
        })
        .into_response(),
        Err(e) => api_error(
            StatusCode::BAD_REQUEST,
            &state.config.log_dir,
            Some(id),
            "delete_sync",
            e,
        ),
    }
}

async fn get_sync_config_handler(State(state): State<Arc<AppState>>) -> Response {
    match load_sync_config(&state.config.sync_config_path) {
        Ok(Some(cfg)) => Json(SyncConfigResponse {
            configured: true,
            config: Some(cfg),
        })
        .into_response(),
        Ok(None) => Json(SyncConfigResponse {
            configured: false,
            config: None,
        })
        .into_response(),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            None,
            "get_sync_config",
            format!("Failed to read sync config: {}", e),
        ),
    }
}

async fn setup_sync_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SyncSetupRequest>,
) -> Response {
    let sync_config = SyncConfig {
        ssh_user: payload.ssh_user,
        ssh_host: payload.ssh_host,
        ssh_port: payload.ssh_port,
        ssh_key_path: payload.ssh_key_path,
        remote_sync_root: payload.remote_sync_root,
        writer_id: None,
        daemon_url: None,
    };

    if payload.verify {
        if let Err(e) = state.sync_manager.verify(&sync_config).await {
            return api_error(
                StatusCode::BAD_REQUEST,
                &state.config.log_dir,
                None,
                "setup_sync",
                format!("Verification failed: {}", e),
            );
        }
    }

    let daemon_url = format!("http://127.0.0.1:{}", state.config.port);
    if let Err(e) = provision_sync_setup(&state.config.sync_config_path, &sync_config, &daemon_url)
    {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            None,
            "setup_sync",
            format!("Failed to save sync config: {}", e),
        );
    }

    Json(json!({
        "status": "ok",
        "message": "Sync configured successfully",
        "config": sync_config
    }))
    .into_response()
}

async fn stop_dsh_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    match state
        .dsh_runtime_manager
        .stop_dsh(&state.config, id, &project.path)
        .await
    {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DSH stopped".to_string()),
        })
        .into_response(),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            Some(id),
            "stop_dsh",
            e,
        ),
    }
}

async fn open_port_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<OpenPortRequest>,
) -> Response {
    if payload.port == 0 {
        return api_error(
            StatusCode::BAD_REQUEST,
            &state.config.log_dir,
            Some(id),
            "open_port",
            "Port must be greater than 0",
        );
    }

    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    let project_host = compute_project_host(&project.path);
    let local_url = format!(
        "http://{}.{}.devvm.localhost:{}",
        payload.port, project_host, state.config.ingress_port
    );
    let tailnet_url = format!(
        "http://{}.{}.{}:{}",
        payload.port, project_host, state.config.tailnet_domain, state.config.ingress_port
    );

    Json(OpenPortResponse {
        local_url,
        tailnet_url,
    })
    .into_response()
}

#[derive(Deserialize)]
struct BrowserParams {
    path: Option<String>,
}

async fn browser_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<BrowserParams>,
) -> Response {
    match browse_directory(&state.config.home_dir, params.path.as_deref()) {
        Ok(result) => Json(result).into_response(),
        Err(BrowserError::AccessDenied(msg)) => api_error(
            StatusCode::FORBIDDEN,
            &state.config.log_dir,
            None,
            "browse_directory",
            msg,
        ),
        Err(BrowserError::PathNotFound(msg)) => api_error(
            StatusCode::NOT_FOUND,
            &state.config.log_dir,
            None,
            "browse_directory",
            msg,
        ),
        Err(BrowserError::NotADirectory(msg)) => api_error(
            StatusCode::BAD_REQUEST,
            &state.config.log_dir,
            None,
            "browse_directory",
            msg,
        ),
        Err(BrowserError::IoError(e)) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &state.config.log_dir,
            None,
            "browse_directory",
            format!("I/O error: {}", e),
        ),
    }
}
