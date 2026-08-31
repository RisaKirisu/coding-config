use crate::browser::{browse_directory, BrowserError};
use crate::config::DaemonConfig;
use crate::logs::read_recent_logs;
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
        .route("/api/projects/{id}/open-port", post(open_port_handler))
        .route(
            "/api/projects/{id}/sync/trigger",
            post(trigger_sync_handler),
        )
        .route("/api/projects/{id}/sync/delete", post(delete_sync_handler))
        .route("/api/sync/config", get(get_sync_config_handler))
        .route("/api/sync/setup", post(setup_sync_handler))
        .route("/api/browser", get(browser_handler))
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
    let dsh_status = state.dsh_runtime_manager.get_status(record.id).await;

    let is_configured = load_sync_config(&state.config.sync_config_path)
        .map(|c| c.is_some())
        .unwrap_or(false);
    let sync_status = state
        .sync_manager
        .get_status(record.id, is_configured)
        .await;

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
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to load projects: {}", e) })),
            )
                .into_response();
        }
    };

    let mut views = Vec::new();
    for rec in &records {
        views.push(build_project_view(&state, rec).await);
    }

    Json(views).into_response()
}

fn get_project_or_response(
    state: &AppState,
    id: Uuid,
) -> Result<ProjectRecord, (StatusCode, Json<serde_json::Value>)> {
    match get_project(&state.config.config_path, id) {
        Ok(Some(p)) => Ok(p),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Project not found" })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Registry error: {}", e) })),
        )),
    }
}

async fn get_project_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return resp.into_response(),
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
        Err(RegistryError::PathNotFound(p)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Path not found: {}", p) })),
        )
            .into_response(),
        Err(RegistryError::NotADirectory(p)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Path is not a directory: {}", p) })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Registration failed: {}", e) })),
        )
            .into_response(),
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
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Project not found in registry" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to unregister: {}", e) })),
        )
            .into_response(),
    }
}

async fn get_logs_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let logs = read_recent_logs(&state.config.log_dir, id, 65536);
    Json(LogsResponse {
        project_id: id,
        logs,
    })
    .into_response()
}

async fn start_vm_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return resp.into_response(),
    };

    match run_vm_start(&state.config, id, &project.path).await {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DevVM started".to_string()),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn stop_vm_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return resp.into_response(),
    };

    state
        .dsh_runtime_manager
        .handle_vm_stopped(&state.config, id)
        .await;

    match run_vm_stop(&state.config, id, &project.path).await {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DevVM stopped".to_string()),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn delete_vm_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return resp.into_response(),
    };

    state
        .dsh_runtime_manager
        .handle_vm_stopped(&state.config, id)
        .await;

    match run_vm_delete(&state.config, id, &project.path).await {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DevVM deleted".to_string()),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn launch_dsh_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return resp.into_response(),
    };

    match state
        .dsh_runtime_manager
        .launch_dsh(&state.config, &state.sync_manager, id, &project.path)
        .await
    {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DSH launched".to_string()),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn trigger_sync_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Response {
    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return resp.into_response(),
    };

    match state
        .sync_manager
        .trigger_sync(&state.config, id, &project.path)
        .await
    {
        Ok(status) => Json(json!({
            "status": "ok",
            "message": "Synchronization triggered",
            "sync_status": status
        }))
        .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn delete_sync_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<SyncDeleteRequest>,
) -> Response {
    if !payload.confirmed {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Confirmation required: set confirmed=true to delete remote sync store" })),
        )
            .into_response();
    }

    let _project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return resp.into_response(),
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
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
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
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to read sync config: {}", e) })),
        )
            .into_response(),
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
    };

    if payload.verify {
        if let Err(e) = state.sync_manager.verify_connection(&sync_config).await {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Verification failed: {}", e) })),
            )
                .into_response();
        }
    }

    if let Err(e) = provision_sync_setup(&state.config.sync_config_path, &sync_config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to save sync config: {}", e) })),
        )
            .into_response();
    }

    Json(json!({
        "status": "ok",
        "message": "Sync configured successfully",
        "config": sync_config
    }))
    .into_response()
}

async fn stop_dsh_handler(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Response {
    match state.dsh_runtime_manager.stop_dsh(&state.config, id).await {
        Ok(()) => Json(ActionResponse {
            status: "ok".to_string(),
            message: Some("DSH stopped".to_string()),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn open_port_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<OpenPortRequest>,
) -> Response {
    if payload.port == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Port must be greater than 0" })),
        )
            .into_response();
    }

    let project = match get_project_or_response(&state, id) {
        Ok(p) => p,
        Err(resp) => return resp.into_response(),
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
        Err(BrowserError::AccessDenied(msg)) => {
            (StatusCode::FORBIDDEN, Json(json!({ "error": msg }))).into_response()
        }
        Err(BrowserError::PathNotFound(msg)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": msg }))).into_response()
        }
        Err(BrowserError::NotADirectory(msg)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
        }
        Err(BrowserError::IoError(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("I/O error: {}", e) })),
        )
            .into_response(),
    }
}
