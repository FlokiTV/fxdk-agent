use std::io;

use fxdk_agent_config::ConfigStore;
use fxdk_agent_control_api::{
    ControlApiState, bind_default, serve_with_state,
};
use tokio::signal;

#[tokio::main]
async fn main() -> io::Result<()> {
    let store = ConfigStore::default_local().map_err(io::Error::other)?;
    let state = ControlApiState::from_store(store).map_err(io::Error::other)?;

    let listener = bind_default().await?;
    let address = listener.local_addr()?;

    println!("FXDK Agent control API listening on http://{address}");

    serve_with_state(listener, state, shutdown_signal()).await
}

async fn shutdown_signal() {
    if let Err(error) = signal::ctrl_c().await {
        eprintln!("failed to listen for Ctrl+C: {error}");
    }
}
