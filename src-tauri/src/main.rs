#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::sync::Arc;

use tauri::{async_runtime, Emitter, Manager};
use tracing_subscriber::EnvFilter;
use y7ke_app::{AppConfig, AppHandle};

const EVENT_CHANNEL: &str = "y7ke://event";

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,y7ke=debug,y7ke_ui=debug")),
        )
        .init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Y7KE");

    // Build the tokio runtime and hand it to Tauri so the libp2p swarm task
    // and Tauri's own async work share the same executor.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    async_runtime::set(runtime.handle().clone());

    let result = tauri::Builder::default()
        .setup(|app| {
            // Boot the y7ke-app composition root synchronously so that all
            // commands have access to a fully-initialised AppHandle the
            // moment the window appears.
            let config = AppConfig::default_for_app()?;
            let y7_handle: AppHandle = async_runtime::block_on(AppHandle::boot(config))?;
            let y7_handle = Arc::new(y7_handle);

            // Forward backend AppEvents to the frontend through a single
            // Tauri event channel. The UI registers one listener and
            // discriminates on `event.kind`.
            let mut sub = y7_handle.subscribe();
            let emitter = app.handle().clone();
            async_runtime::spawn(async move {
                loop {
                    match sub.recv().await {
                        Ok(event) => {
                            if let Err(e) = emitter.emit(EVENT_CHANNEL, &event) {
                                tracing::warn!(error = %e, "failed to emit app event to UI");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("AppEvent channel closed; emitter task exiting");
                            return;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(dropped = n, "AppEvent emitter lagged");
                        }
                    }
                }
            });

            // Make the AppHandle available to commands as managed state.
            app.manage(y7_handle);
            tracing::info!("y7ke shell ready");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Intercept the close so the swarm task can exit cleanly
                // before the runtime tears down. Without this the libp2p
                // task is dropped mid-operation, which leaves the sqlite
                // WAL in a recoverable-but-noisy state on next boot.
                api.prevent_close();
                let win = window.clone();
                async_runtime::spawn(async move {
                    if let Some(handle) = win.try_state::<Arc<AppHandle>>() {
                        let _ = handle.shutdown().await;
                    }
                    // Brief drain window.
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    let _ = win.destroy();
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_my_id,
            commands::list_contacts,
            commands::list_pending_requests,
            commands::send_contact_request,
            commands::accept_request,
            commands::reject_request,
            commands::cancel_request,
            commands::delete_contact,
            commands::list_messages,
            commands::send_message,
            commands::log_from_ui,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "tauri runtime exited with error");
        std::process::exit(1);
    }

    // Hold the runtime alive until process exit.
    drop(runtime);
}
