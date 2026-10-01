use std::io;

use fxdk_agent_control_api::{bind_default, serve};
use tokio::signal;

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = bind_default().await?;
    let address = listener.local_addr()?;

    println!("FXDK Agent control API listening on http://{address}");

    serve(listener, shutdown_signal()).await
}

async fn shutdown_signal() {
    if let Err(error) = signal::ctrl_c().await {
        eprintln!("failed to listen for Ctrl+C: {error}");
    }
}
