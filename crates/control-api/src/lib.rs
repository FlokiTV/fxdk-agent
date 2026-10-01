use std::{
    future::Future,
    io,
    net::{Ipv4Addr, SocketAddr},
};

use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

pub const DEFAULT_CONTROL_PORT: u16 = 35_418;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
}

pub fn default_control_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_CONTROL_PORT))
}

pub async fn bind_default() -> io::Result<TcpListener> {
    TcpListener::bind(default_control_addr()).await
}

pub fn router() -> Router {
    Router::new().route("/v1/health", get(health))
}

pub async fn serve(
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    axum::serve(listener, router())
        .with_graceful_shutdown(shutdown)
        .await
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::{HealthResponse, default_control_addr, router};

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
}
