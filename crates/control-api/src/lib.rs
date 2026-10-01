use std::{
    future::Future,
    io,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderValue, Method, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use fxdk_agent_config::{
    AppConfig, ConfigPatch, ConfigStore, ConfigStoreError, ConfigValidationIssue,
    SyntheticIdentityConfig, SyntheticIdentityPatch, validate_runtime_paths,
};
use fxdk_agent_fxserver::{FxServerController, FxServerError, FxServerPhase, FxServerSnapshot};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::cors::CorsLayer;
use utoipa::{OpenApi, ToSchema};

pub const DEFAULT_CONTROL_PORT: u16 = 35_418;

#[derive(Clone)]
pub struct ControlApiState {
    config: Arc<RwLock<AppConfig>>,
    store: Option<ConfigStore>,
    fxserver: FxServerController,
}

impl ControlApiState {
    pub fn in_memory(config: AppConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            store: None,
            fxserver: FxServerController::default(),
        }
    }

    pub fn from_store(store: ConfigStore) -> Result<Self, ConfigStoreError> {
        let config = store.load_or_create()?;

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            store: Some(store),
            fxserver: FxServerController::default(),
        })
    }

    pub async fn config_snapshot(&self) -> AppConfig {
        self.config.read().await.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorResponse {
    pub ok: bool,
    pub error: ApiErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
    pub detail: Option<ApiErrorContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorContext {
    pub issues: Vec<ConfigValidationIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LauncherState {
    Starting,
    Ready,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ServerState {
    Stopped,
    Starting,
    Online,
    Stopping,
    Crashed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ClientState {
    Stopped,
    Starting,
    Connecting,
    Active,
    Stopping,
    Crashed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Disabled,
    Starting,
    Ready,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LauncherStatus {
    pub state: LauncherState,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub state: ServerState,
    pub address: Option<String>,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub last_error: Option<String>,
    pub log_tail: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClientStatus {
    pub id: u32,
    pub state: ClientState,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AgentStatus {
    pub enabled: bool,
    pub state: AgentState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ControlStatus {
    pub launcher: LauncherStatus,
    pub server: ServerStatus,
    pub clients: Vec<ClientStatus>,
    pub agent: AgentStatus,
}

impl ControlStatus {
    pub fn initial() -> Self {
        Self {
            launcher: LauncherStatus {
                state: LauncherState::Ready,
                pid: Some(std::process::id()),
            },
            server: ServerStatus {
                state: ServerState::Stopped,
                address: None,
                pid: None,
                exit_code: None,
                last_error: None,
                log_tail: Vec::new(),
            },
            clients: Vec::new(),
            agent: AgentStatus {
                enabled: false,
                state: AgentState::Disabled,
            },
        }
    }
}

pub fn default_control_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_CONTROL_PORT))
}

pub async fn bind_default() -> io::Result<TcpListener> {
    TcpListener::bind(default_control_addr()).await
}

pub fn router() -> Router {
    router_with_state(ControlApiState::in_memory(AppConfig::default()))
}

pub fn router_with_state(state: ControlApiState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/status", get(status))
        .route("/v1/server/start", axum::routing::post(start_server))
        .route("/v1/server/stop", axum::routing::post(stop_server))
        .route("/v1/config", get(get_config).patch(patch_config))
        .route("/agent.md", get(agent_guide))
        .route("/openapi.json", get(openapi))
        .layer(desktop_cors())
        .with_state(state)
}

fn desktop_cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://127.0.0.1:1420"),
            HeaderValue::from_static("http://localhost:1420"),
            HeaderValue::from_static("tauri://localhost"),
            HeaderValue::from_static("http://tauri.localhost"),
        ])
        .allow_methods([Method::GET, Method::PATCH, Method::POST])
        .allow_headers([header::CONTENT_TYPE])
}

pub async fn serve(
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    serve_with_state(
        listener,
        ControlApiState::in_memory(AppConfig::default()),
        shutdown,
    )
    .await
}

pub async fn serve_with_state(
    listener: TcpListener,
    state: ControlApiState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    axum::serve(listener, router_with_state(state))
        .with_graceful_shutdown(shutdown)
        .await
}

#[utoipa::path(
    get,
    path = "/v1/health",
    responses((status = 200, description = "Control API health", body = HealthResponse))
)]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

#[utoipa::path(
    get,
    path = "/v1/status",
    responses((status = 200, description = "Current control plane state", body = ControlStatus))
)]
async fn status(State(state): State<ControlApiState>) -> Json<ControlStatus> {
    Json(control_status(&state).await)
}

async fn control_status(state: &ControlApiState) -> ControlStatus {
    let server = state.fxserver.snapshot().await;

    ControlStatus {
        server: server_status(server),
        ..ControlStatus::initial()
    }
}

fn server_status(snapshot: FxServerSnapshot) -> ServerStatus {
    ServerStatus {
        state: match snapshot.phase {
            FxServerPhase::Stopped => ServerState::Stopped,
            FxServerPhase::Starting => ServerState::Starting,
            FxServerPhase::Online => ServerState::Online,
            FxServerPhase::Stopping => ServerState::Stopping,
            FxServerPhase::Crashed => ServerState::Crashed,
        },
        address: snapshot.address,
        pid: snapshot.pid,
        exit_code: snapshot.exit_code,
        last_error: snapshot.last_error,
        log_tail: snapshot.log_tail,
    }
}

type HandlerError = (StatusCode, Json<ApiErrorResponse>);
type ConfigHandlerError = HandlerError;

#[utoipa::path(
    post,
    path = "/v1/server/start",
    responses(
        (status = 200, description = "FXServer reached readiness", body = ControlStatus),
        (status = 409, description = "FXServer is already running", body = ApiErrorResponse),
        (status = 422, description = "FXServer configuration is incomplete", body = ApiErrorResponse),
        (status = 500, description = "FXServer failed to start", body = ApiErrorResponse)
    )
)]
async fn start_server(
    State(state): State<ControlApiState>,
) -> Result<Json<ControlStatus>, HandlerError> {
    let config = state.config_snapshot().await;

    state
        .fxserver
        .start(&config)
        .await
        .map_err(server_start_error)?;

    Ok(Json(control_status(&state).await))
}

#[utoipa::path(
    post,
    path = "/v1/server/stop",
    responses(
        (status = 200, description = "FXServer stopped", body = ControlStatus),
        (status = 500, description = "FXServer failed to stop", body = ApiErrorResponse)
    )
)]
async fn stop_server(
    State(state): State<ControlApiState>,
) -> Result<Json<ControlStatus>, HandlerError> {
    state
        .fxserver
        .stop()
        .await
        .map_err(server_stop_error)?;

    Ok(Json(control_status(&state).await))
}

fn server_start_error(error: FxServerError) -> HandlerError {
    let status = match error {
        FxServerError::AlreadyRunning => StatusCode::CONFLICT,
        FxServerError::MissingServerProject
        | FxServerError::MissingFxServerPath
        | FxServerError::MissingServerConfig(_)
        | FxServerError::InvalidServerProject(_)
        | FxServerError::InvalidFxServerPath(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (
        status,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: if status == StatusCode::CONFLICT {
                    "SERVER_ALREADY_RUNNING".to_owned()
                } else if status == StatusCode::UNPROCESSABLE_ENTITY {
                    "SERVER_CONFIG_INVALID".to_owned()
                } else {
                    "SERVER_START_FAILED".to_owned()
                },
                message: error.to_string(),
                detail: None,
            },
        }),
    )
}

fn server_stop_error(error: FxServerError) -> HandlerError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: "SERVER_STOP_FAILED".to_owned(),
                message: error.to_string(),
                detail: None,
            },
        }),
    )
}

#[utoipa::path(
    get,
    path = "/v1/config",
    responses((status = 200, description = "Sanitized local configuration", body = AppConfig))
)]
async fn get_config(State(state): State<ControlApiState>) -> Json<AppConfig> {
    Json(state.config_snapshot().await)
}

#[utoipa::path(
    patch,
    path = "/v1/config",
    request_body = ConfigPatch,
    responses(
        (status = 200, description = "Updated local configuration", body = AppConfig),
        (status = 422, description = "Configuration validation failed", body = ApiErrorResponse),
        (status = 500, description = "Configuration persistence failed", body = ApiErrorResponse)
    )
)]
async fn patch_config(
    State(state): State<ControlApiState>,
    Json(patch): Json<ConfigPatch>,
) -> Result<Json<AppConfig>, ConfigHandlerError> {
    let mut current = state.config.write().await;
    let mut candidate = current.clone();
    candidate.apply_patch(patch);

    let issues = validate_runtime_paths(&candidate);
    if !issues.is_empty() {
        return Err(config_validation_error(issues));
    }

    if let Some(store) = &state.store
        && store.save(&candidate).is_err()
    {
        return Err(config_persistence_error());
    }

    *current = candidate.clone();
    Ok(Json(candidate))
}

fn config_validation_error(
    issues: Vec<ConfigValidationIssue>,
) -> ConfigHandlerError {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: "CONFIG_VALIDATION_FAILED".to_owned(),
                message: "configuration contains invalid runtime paths".to_owned(),
                detail: Some(ApiErrorContext { issues }),
            },
        }),
    )
}

fn config_persistence_error() -> ConfigHandlerError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: "CONFIG_PERSIST_FAILED".to_owned(),
                message: "failed to persist local configuration".to_owned(),
                detail: None,
            },
        }),
    )
}

#[utoipa::path(
    get,
    path = "/agent.md",
    responses((status = 200, description = "Agent-oriented discovery guide"))
)]
async fn agent_guide() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        include_str!("../../../agent/agent.md"),
    )
}

#[derive(OpenApi)]
#[openapi(
    paths(health, status, start_server, stop_server, get_config, patch_config, agent_guide),
    components(schemas(
        HealthResponse,
        ApiErrorResponse,
        ApiErrorDetail,
        ApiErrorContext,
        AppConfig,
        ConfigPatch,
        ConfigValidationIssue,
        SyntheticIdentityConfig,
        SyntheticIdentityPatch,
        LauncherState,
        ServerState,
        ClientState,
        AgentState,
        LauncherStatus,
        ServerStatus,
        ClientStatus,
        AgentStatus,
        ControlStatus
    )),
    info(
        title = "FXDK Agent Control API",
        version = "0.1.0",
        description = "Local loopback control plane for FXDK Agent."
    )
)]
struct ApiDoc;

async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use std::{fs, net::Ipv4Addr, process};

    use fxdk_agent_config::{AppConfig, ConfigStore};

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::oneshot,
    };
    use tower::ServiceExt;

    use super::{
        AgentState, ApiErrorResponse, ControlApiState, ControlStatus, HealthResponse,
        LauncherState, ServerState, default_control_addr, router, router_with_state, serve,
    };

    fn test_config_store(name: &str) -> ConfigStore {
        let root = std::env::temp_dir().join(format!(
            "fxdk-agent-control-api-test-{}-{name}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        ConfigStore::at(root.join("config.json"))
    }

    #[test]
    fn default_address_is_loopback_only() {
        assert!(default_control_addr().ip().is_loopback());
    }

    #[tokio::test]
    async fn health_route_reports_current_version() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("health response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("health body");
        let payload: HealthResponse = serde_json::from_slice(&body).expect("health json");

        assert!(payload.ok);
        assert_eq!(payload.version, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn status_route_reports_initial_control_plane_state() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/v1/status")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("status response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("status body");
        let payload: ControlStatus = serde_json::from_slice(&body).expect("status json");

        assert_eq!(payload.launcher.state, LauncherState::Ready);
        assert_eq!(payload.launcher.pid, Some(std::process::id()));
        assert_eq!(payload.server.state, ServerState::Stopped);
        assert_eq!(payload.server.address, None);
        assert_eq!(payload.server.pid, None);
        assert!(payload.clients.is_empty());
        assert!(!payload.agent.enabled);
        assert_eq!(payload.agent.state, AgentState::Disabled);
    }

    #[tokio::test]
    async fn agent_guide_is_served_as_markdown() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/agent.md")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("agent guide response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&"text/markdown; charset=utf-8".parse().expect("content type"))
        );

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("agent guide body");
        let text = String::from_utf8(body.to_vec()).expect("agent guide utf8");

        assert!(text.contains("# FXDK Agent — Agent Guide"));
        assert!(text.contains("GET /v1/status"));
        assert!(text.contains("GET /openapi.json"));
    }

    #[tokio::test]
    async fn openapi_route_describes_discovery_endpoints() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("openapi response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("openapi body");
        let document: serde_json::Value =
            serde_json::from_slice(&body).expect("openapi json");

        assert_eq!(document["info"]["title"], "FXDK Agent Control API");
        assert!(document["paths"]["/v1/health"]["get"].is_object());
        assert!(document["paths"]["/v1/status"]["get"].is_object());
        assert!(document["paths"]["/v1/config"]["get"].is_object());
        assert!(document["paths"]["/v1/config"]["patch"].is_object());
        assert!(document["paths"]["/v1/server/start"]["post"].is_object());
        assert!(document["paths"]["/v1/server/stop"]["post"].is_object());
        assert!(document["paths"]["/agent.md"]["get"].is_object());
    }

    #[tokio::test]
    async fn server_start_rejects_missing_configuration() {
        let response = router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/server/start")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("start response");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("start error body");
        let error: ApiErrorResponse = serde_json::from_slice(&body).expect("start error json");
        assert_eq!(error.error.code, "SERVER_CONFIG_INVALID");
    }

    #[tokio::test]
    async fn server_stop_is_idempotent_when_stopped() {
        let response = router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/server/stop")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("stop response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("stop body");
        let status: ControlStatus = serde_json::from_slice(&body).expect("stop json");
        assert_eq!(status.server.state, ServerState::Stopped);
    }

    #[tokio::test]
    async fn config_get_returns_sanitized_default_contract() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/v1/config")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("config response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("config body");
        let config: AppConfig = serde_json::from_slice(&body).expect("config json");

        assert_eq!(config, AppConfig::default());
    }

    #[tokio::test]
    async fn config_patch_persists_valid_runtime_paths() {
        let store = test_config_store("valid-patch");
        let root = store.path().parent().expect("test root");
        let server_project = root.join("server");
        let fxserver_path = root.join("FXServer.exe");
        let fivem_path = root.join("FiveM.exe");

        fs::create_dir_all(&server_project).expect("create server project");
        fs::write(&fxserver_path, b"test").expect("create fxserver");
        fs::write(&fivem_path, b"test").expect("create fivem");

        let state = ControlApiState::from_store(store.clone()).expect("load state");
        let patch = serde_json::json!({
            "serverProject": server_project,
            "fxserverPath": fxserver_path,
            "fivemPath": fivem_path,
            "syntheticIdentity": { "enabled": true }
        });

        let response = router_with_state(state)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/v1/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(patch.to_string()))
                    .expect("request"),
            )
            .await
            .expect("patch response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("patch body");
        let updated: AppConfig = serde_json::from_slice(&body).expect("updated config");

        assert!(updated.synthetic_identity.enabled);
        assert_eq!(store.load().expect("persisted config"), updated);
    }

    #[tokio::test]
    async fn config_patch_rejects_invalid_paths_without_persisting() {
        let store = test_config_store("invalid-patch");
        let state = ControlApiState::from_store(store.clone()).expect("load state");
        let missing = store
            .path()
            .parent()
            .expect("test root")
            .join("missing-server");

        let patch = serde_json::json!({
            "serverProject": missing
        });

        let response = router_with_state(state)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/v1/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(patch.to_string()))
                    .expect("request"),
            )
            .await
            .expect("patch response");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("error body");
        let error: ApiErrorResponse = serde_json::from_slice(&body).expect("error json");

        assert!(!error.ok);
        assert_eq!(error.error.code, "CONFIG_VALIDATION_FAILED");
        assert_eq!(
            error
                .error
                .detail
                .expect("validation detail")
                .issues
                .first()
                .expect("validation issue")
                .field,
            "serverProject"
        );
        assert_eq!(store.load().expect("persisted config"), AppConfig::default());
    }

    #[tokio::test]
    async fn desktop_origin_can_preflight_config_patch() {
        let response = router()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/v1/config")
                    .header(header::ORIGIN, "http://127.0.0.1:1420")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PATCH")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("preflight response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&"http://127.0.0.1:1420".parse().expect("origin header"))
        );
    }

    #[tokio::test]
    async fn server_handles_real_http_and_graceful_shutdown() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind ephemeral listener");
        let address = listener.local_addr().expect("listener address");
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let server = tokio::spawn(async move {
            serve(listener, async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("control api server");
        });

        let mut stream = TcpStream::connect(address).await.expect("connect");
        stream
            .write_all(
                b"GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )
            .await
            .expect("write request");

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .expect("read response");
        let response = String::from_utf8(response).expect("http response utf8");

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("\"ok\":true"));

        shutdown_tx.send(()).expect("signal shutdown");
        server.await.expect("server task join");
    }
}
