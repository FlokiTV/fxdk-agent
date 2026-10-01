use std::{
    fs,
    future::Future,
    io,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use axum::{
    Router,
    http::{HeaderValue, header},
    response::IntoResponse,
    routing::get,
};
use directories::BaseDirs;
use tokio::net::TcpListener;

pub const DEFAULT_RUNTIME_WEB_PORT: u16 = 35_419;

const INDEX_HTML: &str = include_str!("../../../runtime/fxdk/index.html");
const GAME_VIEW_JS: &str = include_str!("../../../runtime/fxdk/game-view.js");
const FXMANIFEST_LUA: &str = include_str!("../../../runtime/fxdk/fxmanifest.lua");
const LAUNCHER_JS: &str = include_str!("../../../runtime/fxdk/launcher.js");

pub fn default_runtime_web_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_RUNTIME_WEB_PORT))
}

pub async fn bind_default() -> io::Result<TcpListener> {
    TcpListener::bind(default_runtime_web_addr()).await
}

pub fn materialize_sdk_root() -> io::Result<PathBuf> {
    let base_dirs = BaseDirs::new()
        .ok_or_else(|| io::Error::other("local data directory is unavailable"))?;
    let root = base_dirs
        .data_local_dir()
        .join("FXDK Agent")
        .join("runtime")
        .join("fxdk");

    materialize_sdk_root_at(&root)?;
    Ok(root)
}

pub fn materialize_sdk_root_at(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)?;
    write_asset(&root.join("fxmanifest.lua"), FXMANIFEST_LUA)?;
    write_asset(&root.join("launcher.js"), LAUNCHER_JS)?;
    write_asset(&root.join("index.html"), INDEX_HTML)?;
    write_asset(&root.join("game-view.js"), GAME_VIEW_JS)?;
    Ok(())
}

fn write_asset(path: &Path, content: &str) -> io::Result<()> {
    if fs::read_to_string(path).is_ok_and(|existing| existing == content) {
        return Ok(());
    }

    fs::write(path, content)
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
    use std::{fs, process};

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;

    use super::{default_runtime_web_addr, materialize_sdk_root_at, router};

    #[test]
    fn runtime_web_is_loopback_only() {
        assert!(default_runtime_web_addr().ip().is_loopback());
    }

    #[test]
    fn materializes_self_contained_sdk_root() {
        let root = std::env::temp_dir().join(format!(
            "fxdk-agent-runtime-web-test-{}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        materialize_sdk_root_at(&root).expect("materialize sdk root");

        for file in ["fxmanifest.lua", "launcher.js", "index.html", "game-view.js"] {
            assert!(root.join(file).is_file(), "{file} must be materialized");
        }

        let launcher = fs::read_to_string(root.join("launcher.js"))
            .expect("launcher js");
        assert!(launcher.contains("sdk:startGame"));
        assert!(launcher.contains("/v1/client/events"));

        let _ = fs::remove_dir_all(root);
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
