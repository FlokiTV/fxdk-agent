use tokio::sync::watch;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let app = tauri::Builder::default()
        .setup(move |_app| {
            let receiver = shutdown_rx.clone();

            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    fxdk_agent_host::run_until(wait_for_shutdown(receiver)).await
                {
                    eprintln!("FXDK Agent embedded host stopped with error: {error}");
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build FXDK Agent desktop shell");

    app.run(move |_app_handle, event| {
        if matches!(
            event,
            tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
        ) {
            let _ = shutdown_tx.send(true);
        }
    });
}

async fn wait_for_shutdown(mut receiver: watch::Receiver<bool>) {
    while !*receiver.borrow() {
        if receiver.changed().await.is_err() {
            break;
        }
    }
}
