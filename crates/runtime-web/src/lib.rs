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
pub const GAME_RESOURCE_NAME: &str = "fxdk-agent-game";
pub const SCREENSHOT_RESOURCE_NAME: &str = "fxdk-agent-screenshot";

const INDEX_HTML: &str = include_str!("../../../runtime/fxdk/index.html");
const GAME_VIEW_JS: &str = include_str!("../../../runtime/fxdk/game-view.js");
const FXMANIFEST_LUA: &str = include_str!("../../../runtime/fxdk/fxmanifest.lua");
const LAUNCHER_JS: &str = include_str!("../../../runtime/fxdk/launcher.js");
const GAME_FXMANIFEST_LUA: &str = include_str!("../../../runtime/fxdk-game/fxmanifest.lua");
const GAME_CLIENT_JS: &str = include_str!("../../../runtime/fxdk-game/agent-client.js");
const SCREENSHOT_FXMANIFEST_LUA: &str =
    include_str!("../../../runtime/fxdk-screenshot/fxmanifest.lua");
const GAME_SCREENSHOT_CLIENT_JS: &str =
    include_str!("../../../runtime/fxdk-screenshot/vendor/screenshot-basic/dist/client.js");
const GAME_SCREENSHOT_UI_HTML: &str =
    include_str!("../../../runtime/fxdk-screenshot/vendor/screenshot-basic/dist/ui.html");
const GAME_SCREENSHOT_LICENSE: &str =
    include_str!("../../../runtime/fxdk-screenshot/vendor/screenshot-basic/LICENSE");
const GAME_SCREENSHOT_UPSTREAM: &str =
    include_str!("../../../runtime/fxdk-screenshot/vendor/screenshot-basic/UPSTREAM.txt");
const GAME_RESOURCE_MARKER: &str = "FXDK Agent managed runtime resource v1\n";
const GAME_RESOURCE_MARKER_FILE: &str = ".fxdk-agent-managed";

pub fn default_runtime_web_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_RUNTIME_WEB_PORT))
}

pub async fn bind_default() -> io::Result<TcpListener> {
    TcpListener::bind(default_runtime_web_addr()).await
}

pub fn materialize_sdk_root() -> io::Result<PathBuf> {
    let base_dirs =
        BaseDirs::new().ok_or_else(|| io::Error::other("local data directory is unavailable"))?;
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

pub fn stage_game_resource(server_project: &Path) -> io::Result<PathBuf> {
    let category = server_project.join("resources").join("[fxdk-agent]");
    let game_resource = category.join(GAME_RESOURCE_NAME);
    let screenshot_resource = category.join(SCREENSHOT_RESOURCE_NAME);

    for resource in [&game_resource, &screenshot_resource] {
        if resource.exists() {
            ensure_managed_resource(resource)?;
        }
    }

    for resource in [&game_resource, &screenshot_resource] {
        if resource.exists() {
            fs::remove_dir_all(resource)?;
        }
    }

    fs::create_dir_all(&game_resource)?;
    write_asset(
        &game_resource.join("fxmanifest.lua"),
        GAME_FXMANIFEST_LUA,
    )?;
    write_asset(
        &game_resource.join("agent-client.js"),
        GAME_CLIENT_JS,
    )?;
    fs::write(
        game_resource.join(GAME_RESOURCE_MARKER_FILE),
        GAME_RESOURCE_MARKER,
    )?;

    fs::create_dir_all(&screenshot_resource)?;
    write_asset(
        &screenshot_resource.join("fxmanifest.lua"),
        SCREENSHOT_FXMANIFEST_LUA,
    )?;
    let screenshot_root = screenshot_resource.join("vendor").join("screenshot-basic");
    let screenshot_dist = screenshot_root.join("dist");
    fs::create_dir_all(&screenshot_dist)?;
    write_asset(
        &screenshot_dist.join("client.js"),
        GAME_SCREENSHOT_CLIENT_JS,
    )?;
    write_asset(
        &screenshot_dist.join("ui.html"),
        GAME_SCREENSHOT_UI_HTML,
    )?;
    write_asset(&screenshot_root.join("LICENSE"), GAME_SCREENSHOT_LICENSE)?;
    write_asset(
        &screenshot_root.join("UPSTREAM.txt"),
        GAME_SCREENSHOT_UPSTREAM,
    )?;
    fs::write(
        screenshot_resource.join(GAME_RESOURCE_MARKER_FILE),
        GAME_RESOURCE_MARKER,
    )?;

    Ok(game_resource)
}

pub fn cleanup_game_resource(server_project: &Path) -> io::Result<()> {
    let category = server_project.join("resources").join("[fxdk-agent]");
    let game_resource = category.join(GAME_RESOURCE_NAME);
    let screenshot_resource = category.join(SCREENSHOT_RESOURCE_NAME);

    for resource in [&game_resource, &screenshot_resource] {
        if resource.exists() {
            ensure_managed_resource(resource)?;
        }
    }

    for resource in [&game_resource, &screenshot_resource] {
        if resource.exists() {
            fs::remove_dir_all(resource)?;
        }
    }

    if category
        .read_dir()
        .is_ok_and(|mut entries| entries.next().is_none())
    {
        fs::remove_dir(category)?;
    }

    Ok(())
}

fn ensure_managed_resource(resource: &Path) -> io::Result<()> {
    let marker = resource.join(GAME_RESOURCE_MARKER_FILE);
    let content = fs::read_to_string(&marker).map_err(|_| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "refusing to replace existing non-managed resource at {}",
                resource.display()
            ),
        )
    })?;

    if content != GAME_RESOURCE_MARKER {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "refusing to replace existing non-managed resource at {}",
                resource.display()
            ),
        ));
    }

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
    use std::{fs, io, process};

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;

    use super::{
        GAME_RESOURCE_NAME, SCREENSHOT_RESOURCE_NAME, cleanup_game_resource,
        default_runtime_web_addr, materialize_sdk_root_at, router, stage_game_resource,
    };

    #[test]
    fn runtime_web_is_loopback_only() {
        assert!(default_runtime_web_addr().ip().is_loopback());
    }

    #[test]
    fn materializes_self_contained_sdk_root() {
        let root =
            std::env::temp_dir().join(format!("fxdk-agent-runtime-web-test-{}", process::id()));
        let _ = fs::remove_dir_all(&root);

        materialize_sdk_root_at(&root).expect("materialize sdk root");

        for file in [
            "fxmanifest.lua",
            "launcher.js",
            "index.html",
            "game-view.js",
        ] {
            assert!(root.join(file).is_file(), "{file} must be materialized");
        }

        let launcher = fs::read_to_string(root.join("launcher.js")).expect("launcher js");
        assert!(launcher.contains("sdk:startGame"));
        assert!(launcher.contains("/v1/client/events"));
        assert!(launcher.contains("/v1/agent/runtime/register"));
        assert!(launcher.contains("/v1/agent/runtime/next"));
        assert!(launcher.contains("/v1/agent/runtime/respond"));
        assert!(launcher.contains("runtime.ping"));
        assert!(launcher.contains("runtime.status"));
        assert!(launcher.contains("sdk:sendGameClientEvent"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stages_and_cleans_managed_game_resource() {
        let root =
            std::env::temp_dir().join(format!("fxdk-agent-game-resource-test-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("server project");

        let resource = stage_game_resource(&root).expect("stage game resource");
        assert_eq!(
            resource.file_name().and_then(|name| name.to_str()),
            Some(GAME_RESOURCE_NAME)
        );
        assert!(resource.join("fxmanifest.lua").is_file());
        let screenshot_resource = root
            .join("resources")
            .join("[fxdk-agent]")
            .join(SCREENSHOT_RESOURCE_NAME);
        assert!(screenshot_resource.join("fxmanifest.lua").is_file());
        assert!(
            screenshot_resource
                .join("vendor")
                .join("screenshot-basic")
                .join("dist")
                .join("client.js")
                .is_file()
        );
        assert!(
            screenshot_resource
                .join("vendor")
                .join("screenshot-basic")
                .join("dist")
                .join("ui.html")
                .is_file()
        );
        assert!(
            screenshot_resource
                .join("vendor")
                .join("screenshot-basic")
                .join("LICENSE")
                .is_file()
        );
        let client = fs::read_to_string(resource.join("agent-client.js")).expect("agent client");
        assert!(client.contains("game.player"));
        assert!(client.contains("game.screenshot"));
        assert!(client.contains("GetGamePool"));

        cleanup_game_resource(&root).expect("cleanup game resources");
        assert!(!resource.exists());
        assert!(!screenshot_resource.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_replace_unmanaged_game_resource() {
        let root = std::env::temp_dir().join(format!(
            "fxdk-agent-game-resource-conflict-test-{}",
            process::id()
        ));
        let resource = root
            .join("resources")
            .join("[fxdk-agent]")
            .join(GAME_RESOURCE_NAME);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&resource).expect("conflict resource");
        fs::write(resource.join("fxmanifest.lua"), "third-party").expect("conflict manifest");

        let error =
            stage_game_resource(&root).expect_err("unmanaged resource must not be replaced");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);

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
            Some(
                &"text/javascript; charset=utf-8"
                    .parse()
                    .expect("content type")
            )
        );
    }
}
