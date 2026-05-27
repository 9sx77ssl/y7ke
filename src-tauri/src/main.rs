#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::sync::Arc;

use tauri::{async_runtime, Emitter, Manager};
use tracing_subscriber::EnvFilter;
use y7ke_app::{AppConfig, AppHandle};

use crate::commands::AppState;

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
            // Register an empty AppState immediately so commands can be
            // resolved; the actual AppHandle::boot runs in the background.
            let state = Arc::new(AppState::new());
            app.manage(Arc::clone(&state));

            let emitter = app.handle().clone();
            async_runtime::spawn(async move {
                let config = match AppConfig::default_for_app() {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!(error = %e, "AppConfig::default_for_app failed");
                        return;
                    }
                };
                let y7_handle: AppHandle = match AppHandle::boot(config).await {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::error!(error = %e, "AppHandle::boot failed");
                        return;
                    }
                };
                let y7_handle = Arc::new(y7_handle);

                // Stream AppEvents from the backend to the UI's single channel.
                let mut sub = y7_handle.subscribe();
                let event_emitter = emitter.clone();
                async_runtime::spawn(async move {
                    loop {
                        match sub.recv().await {
                            Ok(event) => {
                                if let Err(e) = event_emitter.emit(EVENT_CHANNEL, &event) {
                                    tracing::warn!(error = %e, "emit failed");
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(dropped = n, "AppEvent emitter lagged");
                            }
                        }
                    }
                });

                state.set(y7_handle).await;
                let _ = emitter.emit(EVENT_CHANNEL, &serde_json::json!({ "kind": "boot_ready" }));
                tracing::info!("y7ke boot complete; commands now live");
            });

            tracing::info!("y7ke shell ready (boot deferred)");
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
                    if let Some(state) = win.try_state::<Arc<AppState>>() {
                        if let Some(handle) = state.try_get().await {
                            let _ = handle.shutdown().await;
                        }
                    }
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
            commands::boot_ready,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "tauri runtime exited with error");
        std::process::exit(1);
    }

    // Hold the runtime alive until process exit.
    drop(runtime);
}
