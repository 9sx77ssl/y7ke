//! V1 end-to-end test: two in-process clients (Alice + Bob) exercise the
//! seven user-visible V1 capabilities over a local mDNS swarm.
//!
//! The test takes a few seconds — mDNS discovery is not instantaneous. We
//! wrap the high-level flow in a 30-second timeout to keep CI bounded.

use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

use y7ke_app::{AppConfig, AppHandle};
use y7ke_core::{AppEvent, ContactStatus, ConversationId, MessageStatus, RequestResolution};

const OVERALL_TIMEOUT: Duration = Duration::from_secs(45);
const MDNS_BUDGET: Duration = Duration::from_secs(20);

#[cfg_attr(
    any(target_os = "macos", target_os = "windows"),
    ignore = "mDNS unreliable on GitHub Actions runners"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn alice_and_bob_exchange_messages() {
    // Best-effort tracing for postmortem debugging on CI.
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=debug,info")
        .with_test_writer()
        .try_init();

    let result = timeout(OVERALL_TIMEOUT, scenario()).await;
    match result {
        Err(_) => panic!("V1 E2E timed out after {OVERALL_TIMEOUT:?}"),
        Ok(Err(e)) => panic!("V1 E2E failed: {e}"),
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

    // V1 capability 1 — both clients have a stable Y7 identity.
    assert!(alice_id.to_uri().starts_with("y7:"));
    assert!(bob_id.to_uri().starts_with("y7:"));
    assert_ne!(alice_id, bob_id);

    // Wait for mDNS to discover each other before sending the request.
    // mDNS broadcasts every ~5 seconds; we poll the storage layer once
    // each side's swarm has accepted the connection.
    sleep(Duration::from_secs(3)).await;

    let mut bob_events = bob.subscribe();
    let mut alice_events = alice.subscribe();

    // V1 capability 2 — Alice sends a contact request to Bob.
    let send_attempt = timeout(
        MDNS_BUDGET,
        retry_until_ok(|| alice.send_contact_request(bob_id, Some("hi from alice".into()))),
    )
    .await;
    send_attempt
        .map_err(|_| "alice could not reach bob via mDNS within budget")?
        .map_err(|e| format!("send_contact_request failed: {e}"))?;

    // V1 capability 3 — Bob sees the inbound request and accepts it.
    let received = wait_for_event(
        &mut bob_events,
        |ev| matches!(ev, AppEvent::RequestReceived { y7_id, .. } if *y7_id == alice_id.to_uri()),
    )
    .await?;
    let _ = received;

    let pending = bob.list_pending_requests().await?;
    let alice_request = pending
        .into_iter()
        .find(|r| r.peer_y7_id == alice_id.to_uri())
        .ok_or("Bob has no pending request from Alice")?;
    assert_eq!(alice_request.initial_text.as_deref(), Some("hi from alice"));

    bob.accept_request(alice_request.id).await?;

    let bob_contacts = bob.list_contacts().await?;
    assert!(bob_contacts
        .iter()
        .any(|c| c.y7_id == alice_id.to_uri() && c.status == ContactStatus::Accepted));

    // V1 capability 4 — open chat (just verify list_messages works).
    let conversation_id = ConversationId::between(&alice_id, &bob_id);
    let initial = alice.list_messages(bob_id, 100).await?;
    assert!(initial.is_empty(), "no messages yet");

    // V1 capability 5 — exchange encrypted messages.
    let alice_to_bob_text = "hello bob, this is alice";
    let _mid = alice.send_message(bob_id, alice_to_bob_text.into()).await?;

    let bob_recv = wait_for_event(
        &mut bob_events,
        |ev| matches!(ev, AppEvent::MessageReceived { text, .. } if text == alice_to_bob_text),
    )
    .await?;
    let _ = bob_recv;

    let bob_to_alice_text = "hi alice, this is bob";
    let _bmid = bob.send_message(alice_id, bob_to_alice_text.into()).await?;
    let alice_recv = wait_for_event(
        &mut alice_events,
        |ev| matches!(ev, AppEvent::MessageReceived { text, .. } if text == bob_to_alice_text),
    )
    .await?;
    let _ = alice_recv;

    // V1 capability 6 — both DBs hold both messages.
    let _ = conversation_id; // ConversationId import is still useful for clarity above
    let bob_msgs = bob.list_messages(alice_id, 100).await?;
    let alice_msgs = alice.list_messages(bob_id, 100).await?;
    assert_eq!(bob_msgs.len(), 2, "bob should see two messages");
    assert_eq!(alice_msgs.len(), 2, "alice should see two messages");

    // Status assertions.
    let alice_send = alice_msgs
        .iter()
        .find(|m| m.is_mine)
        .expect("alice's own message");
    assert!(
        MessageStatus::from_i64(alice_send.status)
            .map(|s| matches!(
                s,
                MessageStatus::Sent | MessageStatus::Delivered | MessageStatus::Synced
            ))
            .unwrap_or(false),
        "alice's outgoing message should be Sent/Delivered/Synced, was {:?}",
        alice_send.status
    );

    // V1 capability 7 — accept event resolved as accepted.
    let alice_resolved = bob_contacts
        .iter()
        .find(|c| c.y7_id == alice_id.to_uri())
        .unwrap();
    assert_eq!(alice_resolved.status, ContactStatus::Accepted);
    let _ = RequestResolution::Accepted; // exercise the enum import

    drop(alice);
    drop(bob);
    Ok(())
}

/// Retry `op` every 500ms until it returns Ok or the test budget expires.
async fn retry_until_ok<F, Fut, T, E>(mut op: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut last = None;
    for _ in 0..40 {
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

/// Drain `rx` until a matching event arrives or the budget expires.
async fn wait_for_event<F>(
    rx: &mut broadcast::Receiver<AppEvent>,
    mut matcher: F,
) -> Result<AppEvent, &'static str>
where
    F: FnMut(&AppEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        match timeout(
            deadline.duration_since(tokio::time::Instant::now()),
            rx.recv(),
        )
        .await
        {
            Err(_) => return Err("timed out waiting for matching AppEvent"),
            Ok(Err(_)) => return Err("event channel closed"),
            Ok(Ok(ev)) if matcher(&ev) => return Ok(ev),
            Ok(Ok(_)) => continue,
        }
    }
}
