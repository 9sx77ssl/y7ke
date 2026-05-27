//! Tauri command surface — thin wrappers over `y7ke_app::AppHandle`.

use std::sync::Arc;

use tauri::State;
use tokio::sync::Notify;

use y7ke_app::{AppHandle, ContactView, MessageView, RequestView};
use y7ke_core::Y7Id;

/// Slot that holds an AppHandle once boot completes. Commands await
/// `get()` so the window can render before the swarm is fully booted.
pub struct AppState {
    inner: tokio::sync::Mutex<Option<Arc<AppHandle>>>,
    ready: Notify,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: tokio::sync::Mutex::new(None),
            ready: Notify::new(),
        }
    }

    pub async fn set(&self, handle: Arc<AppHandle>) {
        *self.inner.lock().await = Some(handle);
        self.ready.notify_waiters();
    }

    pub async fn get(&self) -> Arc<AppHandle> {
        loop {
            if let Some(h) = self.inner.lock().await.clone() {
                return h;
            }
            self.ready.notified().await;
        }
    }

    pub async fn try_get(&self) -> Option<Arc<AppHandle>> {
        self.inner.lock().await.clone()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

type S<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub async fn get_my_id(app: S<'_>) -> Result<String, String> {
    Ok(app.get().await.my_y7_id().to_uri())
}

#[tauri::command]
pub async fn list_contacts(app: S<'_>) -> Result<Vec<ContactView>, String> {
    app.get().await.list_contacts().await.map_err(err)
}

#[tauri::command]
pub async fn list_pending_requests(app: S<'_>) -> Result<Vec<RequestView>, String> {
    app.get().await.list_pending_requests().await.map_err(err)
}

#[tauri::command]
pub async fn send_contact_request(
    app: S<'_>,
    y7_id: String,
    greeting: Option<String>,
) -> Result<(), String> {
    let peer = Y7Id::parse_strict(&y7_id).map_err(err)?;
    app.get()
        .await
        .send_contact_request(peer, greeting)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn accept_request(app: S<'_>, request_id: i64) -> Result<(), String> {
    app.get()
        .await
        .accept_request(request_id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn reject_request(app: S<'_>, request_id: i64) -> Result<(), String> {
    app.get()
        .await
        .reject_request(request_id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn cancel_request(app: S<'_>, request_id: i64) -> Result<(), String> {
    app.get()
        .await
        .cancel_request(request_id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_contact(app: S<'_>, y7_id: String) -> Result<(), String> {
    let peer = Y7Id::parse_strict(&y7_id).map_err(err)?;
    app.get().await.delete_contact(peer).await.map_err(err)
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

/// UI passes the peer's y7 URI; we derive the 16-byte ConversationId.
#[tauri::command]
pub async fn list_messages(
    app: S<'_>,
    conversation_id: String,
    limit: i64,
) -> Result<Vec<MessageView>, String> {
    let peer = Y7Id::parse_strict(&conversation_id).map_err(err)?;
    app.get()
        .await
        .list_messages(peer, limit)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn send_message(app: S<'_>, to_y7_id: String, text: String) -> Result<String, String> {
    let peer = Y7Id::parse_strict(&to_y7_id).map_err(err)?;
    let mid = app
        .get()
        .await
        .send_message(peer, text)
        .await
        .map_err(err)?;
    Ok(mid.to_string())
}

/// True after AppHandle has booted (UI can stop showing the splashscreen).
#[tauri::command]
pub async fn boot_ready(app: S<'_>) -> Result<bool, String> {
    Ok(app.try_get().await.is_some())
}
