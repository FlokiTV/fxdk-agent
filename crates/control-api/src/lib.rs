use std::{
    future::Future,
    io,
    net::{Ipv4Addr, SocketAddr},
};

use axum::{
    Json, Router,
    http::header,
    response::IntoResponse,
    routing::get,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use utoipa::{OpenApi, ToSchema};

pub const DEFAULT_CONTROL_PORT: u16 = 35_418;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
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
pub struct ServerStatus {
    pub state: ServerState,
    pub address: Option<String>,
    pub pid: Option<u32>,
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
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/status", get(status))
        .route("/agent.md", get(agent_guide))
        .route("/openapi.json", get(openapi))
}

pub async fn serve(
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    axum::serve(listener, router())
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
async fn status() -> Json<ControlStatus> {
    Json(ControlStatus::initial())
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
    paths(health, status, agent_guide),
    components(schemas(
        HealthResponse,
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
    use tower::ServiceExt;

    use super::{
        AgentState, ControlStatus, HealthResponse, LauncherState, ServerState,
        default_control_addr, router,
    };

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
        assert!(document["paths"]["/agent.md"]["get"].is_object());
    }
}
