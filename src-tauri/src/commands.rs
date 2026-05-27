//! Tauri command surface — thin wrappers over `y7ke_app::AppHandle`.
//!
//! Commands accept JSON-friendly inputs (strings rather than typed IDs) so
//! the Svelte side can call them with plain `invoke("name", { camelCase })`
//! syntax. Errors are stringified to keep the IPC layer simple.

use std::sync::Arc;

use tauri::State;

use y7ke_app::{AppHandle, ContactView, MessageView, RequestView};
use y7ke_core::Y7Id;

pub type AppState = Arc<AppHandle>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn get_my_id(app: State<'_, AppState>) -> Result<String, String> {
    Ok(app.my_y7_id().to_uri())
}

#[tauri::command]
pub async fn list_contacts(app: State<'_, AppState>) -> Result<Vec<ContactView>, String> {
    app.list_contacts().await.map_err(err)
}

#[tauri::command]
pub async fn list_pending_requests(app: State<'_, AppState>) -> Result<Vec<RequestView>, String> {
    app.list_pending_requests().await.map_err(err)
}

#[tauri::command]
pub async fn send_contact_request(
    app: State<'_, AppState>,
    y7_id: String,
    greeting: Option<String>,
) -> Result<(), String> {
    let peer = Y7Id::parse_strict(&y7_id).map_err(err)?;
    app.send_contact_request(peer, greeting).await.map_err(err)
}

#[tauri::command]
pub async fn accept_request(app: State<'_, AppState>, request_id: i64) -> Result<(), String> {
    app.accept_request(request_id).await.map_err(err)
}

#[tauri::command]
pub async fn reject_request(app: State<'_, AppState>, request_id: i64) -> Result<(), String> {
    app.reject_request(request_id).await.map_err(err)
}

#[tauri::command]
pub async fn cancel_request(app: State<'_, AppState>, request_id: i64) -> Result<(), String> {
    app.cancel_request(request_id).await.map_err(err)
}

#[tauri::command]
pub async fn delete_contact(app: State<'_, AppState>, y7_id: String) -> Result<(), String> {
    let peer = Y7Id::parse_strict(&y7_id).map_err(err)?;
    app.delete_contact(peer).await.map_err(err)
}

/// Frontend → backend log forwarder. Levels: "trace" | "debug" | "info" | "warn" | "error".
#[tauri::command]
pub fn log_from_ui(level: String, target: String, message: String) {
    match level.as_str() {
        "trace" => tracing::trace!(target: "y7ke_ui", %target, "{message}"),
        "debug" => tracing::debug!(target: "y7ke_ui", %target, "{message}"),
        "info" => tracing::info!(target: "y7ke_ui", %target, "{message}"),
        "warn" => tracing::warn!(target: "y7ke_ui", %target, "{message}"),
        "error" => tracing::error!(target: "y7ke_ui", %target, "{message}"),
        _ => tracing::info!(target: "y7ke_ui", %target, "{message}"),
    }
}

/// The UI sends the peer's y7 URI as the conversation argument (the UI does
/// not know the 16-byte conversation digest). We derive it server-side.
#[tauri::command]
pub async fn list_messages(
    app: State<'_, AppState>,
    conversation_id: String,
    limit: i64,
) -> Result<Vec<MessageView>, String> {
    let peer = Y7Id::parse_strict(&conversation_id).map_err(err)?;
    app.list_messages(peer, limit).await.map_err(err)
}

#[tauri::command]
pub async fn send_message(
    app: State<'_, AppState>,
    to_y7_id: String,
    text: String,
) -> Result<String, String> {
    let peer = Y7Id::parse_strict(&to_y7_id).map_err(err)?;
    let mid = app.send_message(peer, text).await.map_err(err)?;
    Ok(mid.to_string())
}
