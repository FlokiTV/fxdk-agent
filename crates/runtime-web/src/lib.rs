use std::{
    future::Future,
    io,
    net::{Ipv4Addr, SocketAddr},
};

use axum::{
    Router,
    http::{HeaderValue, header},
    response::IntoResponse,
    routing::get,
};
use tokio::net::TcpListener;

pub const DEFAULT_RUNTIME_WEB_PORT: u16 = 35_419;

const INDEX_HTML: &str = include_str!("../../../runtime/fxdk/index.html");
const GAME_VIEW_JS: &str = include_str!("../../../runtime/fxdk/game-view.js");

pub fn default_runtime_web_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_RUNTIME_WEB_PORT))
}

pub async fn bind_default() -> io::Result<TcpListener> {
    TcpListener::bind(default_runtime_web_addr()).await
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/game-view.js", get(game_view))
        .route("/__fxdk-agent/health", get(health))
}

pub async fn serve(
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    axum::serve(listener, router())
        .with_graceful_shutdown(shutdown)
        .await
}

async fn index() -> impl IntoResponse {
    asset("text/html; charset=utf-8", INDEX_HTML)
}

async fn game_view() -> impl IntoResponse {
    asset("text/javascript; charset=utf-8", GAME_VIEW_JS)
}

async fn health() -> &'static str {
    "ok"
}

fn asset(content_type: &'static str, body: &'static str) -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        body,
    )
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;

    use super::{default_runtime_web_addr, router};

    #[test]
    fn runtime_web_is_loopback_only() {
        assert!(default_runtime_web_addr().ip().is_loopback());
    }

    #[tokio::test]
    async fn serves_game_runtime_shell_and_script() {
        let index = router()
            .oneshot(
                Request::builder()
                    .uri("/?client=1&server=127.0.0.1:30120")
                    .body(Body::empty())
                    .expect("index request"),
            )
            .await
            .expect("index response");

        assert_eq!(index.status(), StatusCode::OK);
        assert_eq!(
            index.headers().get(header::CACHE_CONTROL),
            Some(&"no-store".parse().expect("cache header"))
        );
        let body = to_bytes(index.into_body(), usize::MAX)
            .await
            .expect("index body");
        assert!(String::from_utf8_lossy(&body).contains("FXDK Agent Runtime"));

        let script = router()
            .oneshot(
                Request::builder()
                    .uri("/game-view.js")
                    .body(Body::empty())
                    .expect("script request"),
            )
            .await
            .expect("script response");

        assert_eq!(script.status(), StatusCode::OK);
        assert_eq!(
            script.headers().get(header::CONTENT_TYPE),
            Some(&"text/javascript; charset=utf-8".parse().expect("content type"))
        );
    }
}
