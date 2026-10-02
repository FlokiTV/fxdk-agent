use std::io;

use tokio::signal;

#[tokio::main]
async fn main() -> io::Result<()> {
    fxdk_agent_host::run_until(shutdown_signal()).await
}

async fn shutdown_signal() {
    if let Err(error) = signal::ctrl_c().await {
        eprintln!("failed to listen for Ctrl+C: {error}");
    }
}
