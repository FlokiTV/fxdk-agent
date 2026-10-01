use std::{
    env, fs, net::TcpListener as StdTcpListener, path::PathBuf, process,
    time::Duration,
};

use fxdk_agent_config::AppConfig;
use fxdk_agent_fxserver::{FxServerController, FxServerPhase};

#[tokio::test]
#[ignore = "requires FXDK_AGENT_TEST_FXSERVER pointing to a local FXServer.exe"]
async fn real_fxserver_reaches_readiness_and_stops_cleanly() {
    let executable = env::var_os("FXDK_AGENT_TEST_FXSERVER")
        .map(PathBuf::from)
        .expect("FXDK_AGENT_TEST_FXSERVER is required");
    assert!(executable.is_file(), "FXServer executable must exist");

    let port = reserve_loopback_port();
    let address = format!("127.0.0.1:{port}");
    let root = env::temp_dir().join(format!(
        "fxdk-agent-real-fxserver-{}-{port}",
        process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create temporary server-data");

    fs::write(
        root.join("server.cfg"),
        format!(
            r#"endpoint_add_tcp "{address}"
endpoint_add_udp "{address}"
sv_hostname "FXDK Agent lifecycle smoke"
sv_maxclients 2
sv_lan 1
set onesync on
"#
        ),
    )
    .expect("write temporary server.cfg");

    let config = AppConfig {
        server_project: Some(root.clone()),
        fxserver_path: Some(executable),
        server_address: address,
        ..AppConfig::default()
    };
    let controller = FxServerController::new(Duration::from_secs(45));

    let started = match controller.start(&config).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let snapshot = controller.snapshot().await;
            panic!(
                "FXServer failed to start: {error}\nlogs:\n{}",
                snapshot.log_tail.join("\n")
            );
        }
    };

    assert_eq!(started.phase, FxServerPhase::Online);
    assert!(started.pid.is_some());

    let stopped = controller.stop().await.expect("stop real FXServer");
    assert_eq!(stopped.phase, FxServerPhase::Stopped);
    assert_eq!(stopped.pid, None);

    let restarted = controller.start(&config).await.expect("restart real FXServer");
    assert_eq!(restarted.phase, FxServerPhase::Online);
    assert!(restarted.pid.is_some());

    let restopped = controller.stop().await.expect("stop restarted FXServer");
    assert_eq!(restopped.phase, FxServerPhase::Stopped);
    assert_eq!(restopped.pid, None);

    let _ = fs::remove_dir_all(root);
}

fn reserve_loopback_port() -> u16 {
    let listener = StdTcpListener::bind(("127.0.0.1", 0)).expect("reserve test port");
    let port = listener.local_addr().expect("test address").port();
    drop(listener);
    port
}
