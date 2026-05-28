//! Security regression: a Blocked peer's inbound traffic is dropped.
//!
//! reject_request marks the contact Blocked but keeps the session row, so
//! before the handle_msg block gate a blocked peer could still deliver
//! text — or ride a control frame (AcceptedRequest / ChatDeleted) inside
//! /y7ke/msg to un-block itself or wipe the conversation. This proves the
//! message never surfaces and the block stays put.

use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

use y7ke_app::{AppConfig, AppHandle};
use y7ke_core::{AppEvent, ContactStatus};

const OVERALL_TIMEOUT: Duration = Duration::from_secs(180);
const MDNS_BUDGET: Duration = Duration::from_secs(40);

#[cfg_attr(
    any(target_os = "macos", target_os = "windows"),
    ignore = "mDNS unreliable on GitHub Actions runners"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blocked_peer_inbound_is_dropped() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=info")
        .with_test_writer()
        .try_init();
    match timeout(OVERALL_TIMEOUT, scenario()).await {
        Err(_) => panic!("block_enforcement timed out"),
        Ok(Err(e)) => panic!("scenario failed: {e}"),
        Ok(Ok(())) => {}
    }
}

async fn scenario() -> Result<(), Box<dyn std::error::Error>> {
    let alice_dir = TempDir::new()?;
    let bob_dir = TempDir::new()?;
    let alice = AppHandle::boot(AppConfig::in_dir(alice_dir.path())).await?;
    let bob = AppHandle::boot(AppConfig::in_dir(bob_dir.path())).await?;
    let alice_id = *alice.my_y7_id();
    let bob_id = *bob.my_y7_id();
    sleep(Duration::from_secs(3)).await;

    // Bob requests Alice (this handshakes → both get sessions; Alice gets
    // a PendingIn request). Alice then REJECTS → Bob becomes Blocked.
    let mut alice_events = alice.subscribe();
    timeout(
        MDNS_BUDGET,
        retry_until_ok(|| bob.send_contact_request(alice_id, Some("let me in".into()))),
    )
    .await??;
    wait_for_event(
        &mut alice_events,
        |ev| matches!(ev, AppEvent::RequestReceived { y7_id, .. } if *y7_id == bob_id.to_uri()),
    )
    .await?;
    let req = alice
        .list_pending_requests()
        .await?
        .into_iter()
        .find(|r| r.peer_y7_id == bob_id.to_uri())
        .ok_or("no pending request on alice")?;
    alice.reject_request(req.id).await?;
    assert_eq!(contact_status(&alice, &bob_id).await?, Some(ContactStatus::Blocked));

    // Bob still holds the session, so send_message succeeds locally and the
    // envelope reaches Alice's handle_msg. It must be dropped silently.
    let mut alice_events = alice.subscribe();
    bob.send_message(alice_id, "blocked payload".into()).await?;

    // No MessageReceived from Bob should arrive within the window.
    let leaked = timeout(Duration::from_secs(8), async {
        loop {
            match alice_events.recv().await {
                Ok(AppEvent::MessageReceived { sender_y7_id, text, .. })
                    if sender_y7_id == bob_id.to_uri() && text == "blocked payload" =>
                {
                    return true;
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(!leaked, "blocked peer's message surfaced to Alice");

    // The block must still hold (no control frame re-promoted Bob) and the
    // message must not be persisted.
    assert_eq!(
        contact_status(&alice, &bob_id).await?,
        Some(ContactStatus::Blocked),
        "block was undone"
    );
    let from_bob = alice
        .list_messages(bob_id, 1000)
        .await?
        .into_iter()
        .filter(|m| m.sender_y7_id == bob_id.to_uri())
        .count();
    assert_eq!(from_bob, 0, "blocked peer's message was persisted");
    Ok(())
}

async fn contact_status(
    app: &AppHandle,
    peer: &y7ke_core::Y7Id,
) -> Result<Option<ContactStatus>, Box<dyn std::error::Error>> {
    Ok(app
        .list_contacts()
        .await?
        .into_iter()
        .find(|c| c.y7_id == peer.to_uri())
        .map(|c| c.status))
}

async fn retry_until_ok<F, Fut, T, E>(mut op: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut last = None;
    for _ in 0..60 {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = Some(e);
                sleep(Duration::from_millis(500)).await;
            }
        }
    }
    Err(last.expect("retry never ran"))
}

async fn wait_for_event<F>(
    rx: &mut broadcast::Receiver<AppEvent>,
    mut matcher: F,
) -> Result<AppEvent, &'static str>
where
    F: FnMut(&AppEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match timeout(deadline.duration_since(tokio::time::Instant::now()), rx.recv()).await {
            Err(_) => return Err("timed out waiting for matching AppEvent"),
            Ok(Err(_)) => return Err("event channel closed"),
            Ok(Ok(ev)) if matcher(&ev) => return Ok(ev),
            Ok(Ok(_)) => continue,
        }
    }
}
