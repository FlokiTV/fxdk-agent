use std::{future::Future, io};

use fxdk_agent_config::ConfigStore;
use fxdk_agent_control_api::{
    ControlApiState, bind_default as bind_control_api, serve_with_state,
};
use fxdk_agent_runtime_web::{
    bind_default as bind_runtime_web, serve as serve_runtime_web,
};
use tokio::sync::watch;

pub async fn run_until(
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    let store = ConfigStore::default_local().map_err(io::Error::other)?;
    let state = ControlApiState::from_store(store).map_err(io::Error::other)?;

    let control_listener = bind_control_api().await?;
    let runtime_listener = bind_runtime_web().await?;
    let control_address = control_listener.local_addr()?;
    let runtime_address = runtime_listener.local_addr()?;

    println!("FXDK Agent control API listening on http://{control_address}");
    println!("FXDK Agent runtime web listening on http://{runtime_address}");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shutdown_task = tokio::spawn(async move {
        shutdown.await;
        let _ = shutdown_tx.send(true);
    });

    let control_shutdown = wait_for_shutdown(shutdown_rx.clone());
    let runtime_shutdown = wait_for_shutdown(shutdown_rx);

    let (control_result, runtime_result) = tokio::join!(
        serve_with_state(control_listener, state, control_shutdown),
        serve_runtime_web(runtime_listener, runtime_shutdown),
    );

    shutdown_task.abort();
    control_result?;
    runtime_result?;

    Ok(())
}

async fn wait_for_shutdown(mut receiver: watch::Receiver<bool>) {
    while !*receiver.borrow() {
        if receiver.changed().await.is_err() {
            break;
        }
    }
}
