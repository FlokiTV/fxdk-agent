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
use fxdk_agent_dev_identity::{DevIdentityError, DevIdentityStore};
use fxdk_agent_fivem_client::{
    FivemClientController, FivemClientError, FivemClientEvent, FivemClientPhase,
    FivemClientSnapshot,
};
use fxdk_agent_fxserver::{
    FxServerController, FxServerError, FxServerPhase, FxServerSnapshot, FxServerStartOptions,
};
use fxdk_agent_runtime_web::materialize_sdk_root;
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::cors::CorsLayer;
use utoipa::{OpenApi, ToSchema};

pub const DEFAULT_CONTROL_PORT: u16 = 35_418;

#[derive(Clone)]
pub struct ControlApiState {
    config: Arc<RwLock<AppConfig>>,
    store: Option<ConfigStore>,
    identity_store: Option<DevIdentityStore>,
    fxserver: FxServerController,
    client: FivemClientController,
}

impl ControlApiState {
    pub fn in_memory(config: AppConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            store: None,
            identity_store: None,
            fxserver: FxServerController::default(),
            client: FivemClientController::default(),
        }
    }

    pub fn from_store(store: ConfigStore) -> Result<Self, ConfigStoreError> {
        let config = store.load_or_create()?;

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            store: Some(store),
            identity_store: None,
            fxserver: FxServerController::default(),
            client: FivemClientController::default(),
        })
    }

    pub async fn config_snapshot(&self) -> AppConfig {
        self.config.read().await.clone()
    }

    pub fn with_identity_store(mut self, store: DevIdentityStore) -> Self {
        self.identity_store = Some(store);
        self
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
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub id: u32,
    pub state: ClientState,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub last_error: Option<String>,
    pub connection_state: Option<u32>,
    pub game_process_state: Option<u32>,
    pub log_tail: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClientRequest {
    pub client: u32,
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
        .route("/v1/client/start", axum::routing::post(start_client))
        .route("/v1/client/stop", axum::routing::post(stop_client))
        .route("/v1/client/events", axum::routing::post(client_event))
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
    let client = state.client.snapshot().await;
    let clients = if client.phase == FivemClientPhase::Stopped {
        Vec::new()
    } else {
        vec![client_status(client)]
    };

    ControlStatus {
        server: server_status(server),
        clients,
        ..ControlStatus::initial()
    }
}

fn client_status(snapshot: FivemClientSnapshot) -> ClientStatus {
    ClientStatus {
        id: snapshot.id,
        state: match snapshot.phase {
            FivemClientPhase::Stopped => ClientState::Stopped,
            FivemClientPhase::Starting => ClientState::Starting,
            FivemClientPhase::Connecting => ClientState::Connecting,
            FivemClientPhase::Active => ClientState::Active,
            FivemClientPhase::Stopping => ClientState::Stopping,
            FivemClientPhase::Crashed => ClientState::Crashed,
        },
        pid: snapshot.pid,
        exit_code: snapshot.exit_code,
        last_error: snapshot.last_error,
        connection_state: snapshot.connection_state,
        game_process_state: snapshot.game_process_state,
        log_tail: snapshot.log_tail,
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
    let options = server_start_options(&state, &config)?;

    state
        .fxserver
        .start_with_options(&config, options)
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

fn server_start_options(
    state: &ControlApiState,
    config: &AppConfig,
) -> Result<FxServerStartOptions, HandlerError> {
    if !config.synthetic_identity.enabled {
        return Ok(FxServerStartOptions::default());
    }

    let store = match &state.identity_store {
        Some(store) => store.clone(),
        None => DevIdentityStore::default_local().map_err(dev_identity_error)?,
    };
    let identity = store
        .identity_for_slot(state.client.id())
        .map_err(dev_identity_error)?;

    Ok(FxServerStartOptions {
        synthetic_identity: Some(identity),
    })
}

fn dev_identity_error(error: DevIdentityError) -> HandlerError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: "DEV_IDENTITY_FAILED".to_owned(),
                message: error.to_string(),
                detail: None,
            },
        }),
    )
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
    post,
    path = "/v1/client/start",
    request_body = ClientRequest,
    responses(
        (status = 200, description = "FiveM/FxDK client started", body = ControlStatus),
        (status = 409, description = "Server not online or client already running", body = ApiErrorResponse),
        (status = 422, description = "Client configuration is invalid", body = ApiErrorResponse),
        (status = 500, description = "FiveM/FxDK client failed to start", body = ApiErrorResponse)
    )
)]
async fn start_client(
    State(state): State<ControlApiState>,
    Json(request): Json<ClientRequest>,
) -> Result<Json<ControlStatus>, HandlerError> {
    validate_client_slot(&state, request.client)?;

    let server = state.fxserver.snapshot().await;
    if server.phase != FxServerPhase::Online {
        return Err(client_server_not_online_error());
    }

    let sdk_root = materialize_sdk_root().map_err(client_runtime_prepare_error)?;
    let config = state.config_snapshot().await;

    state
        .client
        .start(&config, &sdk_root)
        .await
        .map_err(client_start_error)?;

    Ok(Json(control_status(&state).await))
}

#[utoipa::path(
    post,
    path = "/v1/client/stop",
    request_body = ClientRequest,
    responses(
        (status = 200, description = "FiveM/FxDK client stopped", body = ControlStatus),
        (status = 422, description = "Client slot is unsupported", body = ApiErrorResponse),
        (status = 500, description = "FiveM/FxDK client failed to stop", body = ApiErrorResponse)
    )
)]
async fn stop_client(
    State(state): State<ControlApiState>,
    Json(request): Json<ClientRequest>,
) -> Result<Json<ControlStatus>, HandlerError> {
    validate_client_slot(&state, request.client)?;

    state
        .client
        .stop()
        .await
        .map_err(client_stop_error)?;

    Ok(Json(control_status(&state).await))
}

fn validate_client_slot(
    state: &ControlApiState,
    client: u32,
) -> Result<(), HandlerError> {
    if client == state.client.id() {
        return Ok(());
    }

    Err((
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: "CLIENT_SLOT_UNSUPPORTED".to_owned(),
                message: format!("client slot {client} is not supported by this MVP"),
                detail: None,
            },
        }),
    ))
}

fn client_server_not_online_error() -> HandlerError {
    (
        StatusCode::CONFLICT,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: "CLIENT_SERVER_NOT_ONLINE".to_owned(),
                message: "FXServer must be online before starting a client".to_owned(),
                detail: None,
            },
        }),
    )
}

fn client_runtime_prepare_error(error: io::Error) -> HandlerError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: "CLIENT_RUNTIME_PREPARE_FAILED".to_owned(),
                message: error.to_string(),
                detail: None,
            },
        }),
    )
}

fn client_start_error(error: FivemClientError) -> HandlerError {
    let status = match error {
        FivemClientError::AlreadyRunning => StatusCode::CONFLICT,
        FivemClientError::MissingFiveMPath
        | FivemClientError::InvalidFiveMPath(_)
        | FivemClientError::InvalidSdkRoot(_)
        | FivemClientError::WrongClientId { .. } => StatusCode::UNPROCESSABLE_ENTITY,
        FivemClientError::Spawn(_) | FivemClientError::Stop(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };

    (
        status,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: if status == StatusCode::CONFLICT {
                    "CLIENT_ALREADY_RUNNING".to_owned()
                } else if status == StatusCode::UNPROCESSABLE_ENTITY {
                    "CLIENT_CONFIG_INVALID".to_owned()
                } else {
                    "CLIENT_START_FAILED".to_owned()
                },
                message: error.to_string(),
                detail: None,
            },
        }),
    )
}

fn client_stop_error(error: FivemClientError) -> HandlerError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: "CLIENT_STOP_FAILED".to_owned(),
                message: error.to_string(),
                detail: None,
            },
        }),
    )
}

#[utoipa::path(
    post,
    path = "/v1/client/events",
    request_body = FivemClientEvent,
    responses(
        (status = 204, description = "Client runtime event accepted"),
        (status = 422, description = "Client runtime event is invalid", body = ApiErrorResponse)
    )
)]
async fn client_event(
    State(state): State<ControlApiState>,
    Json(event): Json<FivemClientEvent>,
) -> Result<StatusCode, HandlerError> {
    state
        .client
        .record_event(event)
        .await
        .map_err(client_event_error)?;

    Ok(StatusCode::NO_CONTENT)
}

fn client_event_error(error: FivemClientError) -> HandlerError {
    let status = if matches!(error, FivemClientError::WrongClientId { .. }) {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };

    (
        status,
        Json(ApiErrorResponse {
            ok: false,
            error: ApiErrorDetail {
                code: if status == StatusCode::UNPROCESSABLE_ENTITY {
                    "CLIENT_EVENT_INVALID".to_owned()
                } else {
                    "CLIENT_EVENT_FAILED".to_owned()
                },
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
    paths(health, status, start_server, stop_server, start_client, stop_client, client_event, get_config, patch_config, agent_guide),
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
        ClientRequest,
        AgentStatus,
        ControlStatus,
        FivemClientEvent
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

    use fxdk_agent_config::{AppConfig, ConfigStore, SyntheticIdentityConfig};
    use fxdk_agent_dev_identity::DevIdentityStore;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::oneshot,
    };
    use tower::ServiceExt;

    use super::{
        AgentState, ApiErrorResponse, ControlApiState, ControlStatus, HealthResponse,
        LauncherState, ServerState, default_control_addr, router, router_with_state,
        server_start_options, serve,
    };

    fn test_config_store(name: &str) -> ConfigStore {
        let root = std::env::temp_dir().join(format!(
            "fxdk-agent-control-api-test-{}-{name}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        ConfigStore::at(root.join("config.json"))
    }

    fn test_identity_store(name: &str) -> DevIdentityStore {
        let root = std::env::temp_dir().join(format!(
            "fxdk-agent-control-api-identity-test-{}-{name}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        DevIdentityStore::at(root.join("identity.json"))
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
        assert!(document["paths"]["/v1/client/start"]["post"].is_object());
        assert!(document["paths"]["/v1/client/stop"]["post"].is_object());
        assert!(document["paths"]["/v1/client/events"]["post"].is_object());
        assert!(document["paths"]["/agent.md"]["get"].is_object());
    }

    #[test]
    fn server_start_options_skip_identity_when_disabled() {
        let store = test_identity_store("disabled");
        let path = store.path().to_path_buf();
        let state = ControlApiState::in_memory(AppConfig::default())
            .with_identity_store(store);

        let options = server_start_options(&state, &AppConfig::default())
            .expect("server start options");

        assert!(options.synthetic_identity.is_none());
        assert!(!path.exists());
    }

    #[test]
    fn server_start_options_use_stable_slot_one_identity_when_enabled() {
        let store = test_identity_store("enabled");
        let config = AppConfig {
            synthetic_identity: SyntheticIdentityConfig { enabled: true },
            ..AppConfig::default()
        };
        let state = ControlApiState::in_memory(config.clone())
            .with_identity_store(store.clone());

        let first = server_start_options(&state, &config)
            .expect("first identity")
            .synthetic_identity
            .expect("synthetic identity");
        let second = server_start_options(&state, &config)
            .expect("second identity")
            .synthetic_identity
            .expect("synthetic identity");

        assert_eq!(first.slot, 1);
        assert_eq!(first, second);
        assert_eq!(first.payload.len(), 40);
        assert!(first.payload.starts_with("deadbeef"));
        assert!(store.path().is_file());
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
    async fn client_start_requires_online_server() {
        let response = router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/client/start")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"client":1}"#))
                    .expect("request"),
            )
            .await
            .expect("client start response");

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("client start error body");
        let error: ApiErrorResponse =
            serde_json::from_slice(&body).expect("client start error json");
        assert_eq!(error.error.code, "CLIENT_SERVER_NOT_ONLINE");
    }

    #[tokio::test]
    async fn client_stop_is_idempotent_when_stopped() {
        let response = router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/client/stop")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"client":1}"#))
                    .expect("request"),
            )
            .await
            .expect("client stop response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("client stop body");
        let status: ControlStatus =
            serde_json::from_slice(&body).expect("client stop json");
        assert!(status.clients.is_empty());
    }

    #[tokio::test]
    async fn client_slot_two_is_rejected_in_mvp() {
        let response = router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/client/stop")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"client":2}"#))
                    .expect("request"),
            )
            .await
            .expect("client stop response");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn client_event_accepts_launcher_heartbeat() {
        let payload = serde_json::json!({
            "clientId": 1,
            "kind": "heartbeat",
            "gameProcessState": 2,
            "connectionState": 0,
            "active": false
        });

        let response = router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/client/events")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("request"),
            )
            .await
            .expect("client event response");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn client_event_rejects_other_client_slot() {
        let payload = serde_json::json!({
            "clientId": 2,
            "kind": "heartbeat",
            "active": false
        });

        let response = router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/client/events")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("request"),
            )
            .await
            .expect("client event response");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
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
