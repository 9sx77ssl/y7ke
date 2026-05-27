//! Sending a message immediately after `send_contact_request`, before the
//! responder accepts. Proves the "send while pending_out" flow lands the
//! message locally (status=Sending), survives a `list_messages` round-trip,
//! and is delivered to the responder.

use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

use y7ke_app::{AppConfig, AppHandle};
use y7ke_core::{AppEvent, ContactStatus, MessageStatus};

const OVERALL_TIMEOUT: Duration = Duration::from_secs(60);
const MDNS_BUDGET: Duration = Duration::from_secs(20);

#[cfg_attr(
    any(target_os = "macos", target_os = "windows"),
    ignore = "mDNS unreliable on GitHub Actions runners"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_message_while_pending_out_lands_locally_and_remotely() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=debug,info")
        .with_test_writer()
        .try_init();

    let result = timeout(OVERALL_TIMEOUT, scenario()).await;
    match result {
        Err(_) => panic!("send-while-pending-out test timed out after {OVERALL_TIMEOUT:?}"),
        Ok(Err(e)) => panic!("send-while-pending-out failed: {e}"),
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

    let mut bob_events = bob.subscribe();

    // 1. Alice sends a contact request. Handshake completes => both sides
    //    hold a session, alice = PendingOut for bob, bob = PendingIn for alice.
    timeout(
        MDNS_BUDGET,
        retry_until_ok(|| alice.send_contact_request(bob_id, Some("hi".into()))),
    )
    .await??;

    // 2. Immediately — before bob accepts — alice sends a message.
    let early_text = "msg before accept";
    let mid = alice.send_message(bob_id, early_text.into()).await?;

    // 3. Locally on alice, the message is persisted. status starts at Sending
    //    but the bg push may flip it to Delivered before we observe — accept
    //    Sending / Sent / Delivered / Synced (none should be Failed).
    let alice_msgs = alice.list_messages(bob_id, 100).await?;
    let row = alice_msgs
        .iter()
        .find(|m| m.message_id == mid.to_string())
        .ok_or("alice's own message missing from list_messages")?;
    assert!(row.is_mine, "row should be marked is_mine");
    assert_eq!(row.text, early_text);
    let status = MessageStatus::from_i64(row.status).ok_or("invalid status discriminant")?;
    assert!(
        matches!(
            status,
            MessageStatus::Sending
                | MessageStatus::Sent
                | MessageStatus::Delivered
                | MessageStatus::Synced
        ),
        "alice's outgoing message status must not be Failed; was {status:?}"
    );

    // 4. Bob receives MessageReceived even though he hasn't accepted yet —
    //    that's the intentional behavior (pending_in users can still receive
    //    messages; the contact stays gated until accept_request).
    wait_for_event(
        &mut bob_events,
        |ev| matches!(ev, AppEvent::MessageReceived { text, .. } if text == early_text),
    )
    .await?;

    // 5. Bob accepts. Alice is auto-promoted on bob's side; alice's
    //    PendingOut should flip to Accepted via the AcceptedRequest control.
    let pending = bob.list_pending_requests().await?;
    let alice_req = pending
        .into_iter()
        .find(|r| r.peer_y7_id == alice_id.to_uri())
        .ok_or("bob missing alice's pending request")?;
    bob.accept_request(alice_req.id).await?;

    // 6. Bob's list_messages now shows the early message.
    let bob_msgs = bob.list_messages(alice_id, 100).await?;
    assert!(
        bob_msgs.iter().any(|m| m.text == early_text),
        "bob's list_messages should include the early message"
    );

    // 7. Alice's contact for bob becomes Accepted (via the AcceptedRequest
    //    control coming back from bob). Generous budget; the control is
    //    fire-and-forget but goes over the existing session immediately.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let contacts = alice.list_contacts().await?;
        let entry = contacts.iter().find(|c| c.y7_id == bob_id.to_uri());
        if let Some(c) = entry {
            if c.status == ContactStatus::Accepted {
                break;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            let actual = entry.map(|c| c.status);
            return Err(format!(
                "alice's contact for bob never reached Accepted; latest: {actual:?}"
            )
            .into());
        }
        sleep(Duration::from_millis(200)).await;
    }

    // 8. Send another message *after* acceptance to prove the steady-state
    //    path still works.
    let post_text = "msg after accept";
    alice.send_message(bob_id, post_text.into()).await?;
    wait_for_event(
        &mut bob_events,
        |ev| matches!(ev, AppEvent::MessageReceived { text, .. } if text == post_text),
    )
    .await?;

    let bob_final = bob.list_messages(alice_id, 100).await?;
    assert_eq!(
        bob_final.iter().filter(|m| !m.is_mine).count(),
        2,
        "bob should hold both inbound messages"
    );

    Ok(())
}

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
