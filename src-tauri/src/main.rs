#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,y7ke=debug")),
        )
        .init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Y7KE");

    tauri::Builder::default()
        .setup(|_app| Ok(()))
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
