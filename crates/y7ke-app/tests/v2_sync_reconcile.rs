//! V2-C1: /y7ke/sync/1.0.0 reconcile delivers offline messages with queue wiped.

use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

use y7ke_app::{AppConfig, AppHandle};
use y7ke_core::AppEvent;

const OVERALL_TIMEOUT: Duration = Duration::from_secs(180);
const MDNS_BUDGET: Duration = Duration::from_secs(40);

#[cfg_attr(
    any(target_os = "macos", target_os = "windows"),
    ignore = "mDNS unreliable on GitHub Actions runners"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_reconcile_delivers_offline_messages() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=debug,info")
        .with_test_writer()
        .try_init();
    match timeout(OVERALL_TIMEOUT, scenario()).await {
        Err(_) => panic!("sync_reconcile timed out"),
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

    // Handshake.
    let mut bob_events = bob.subscribe();
    timeout(
        MDNS_BUDGET,
        retry_until_ok(|| alice.send_contact_request(bob_id, Some("sync reconcile".into()))),
    )
    .await??;
    wait_for_event(
        &mut bob_events,
        |ev| matches!(ev, AppEvent::RequestReceived { y7_id, .. } if *y7_id == alice_id.to_uri()),
    )
    .await?;
    let pending = bob.list_pending_requests().await?;
    let req = pending
        .into_iter()
        .find(|r| r.peer_y7_id == alice_id.to_uri())
        .ok_or("no pending on bob")?;
    bob.accept_request(req.id).await?;

    // Live delivery: Alice sends, Bob receives. Confirms the session is
    // established on both sides before we tear Bob down.
    alice.send_message(bob_id, "live".into()).await?;
    wait_for_event(&mut bob_events, |ev| {
        matches!(ev, AppEvent::MessageReceived { sender_y7_id, text, .. }
            if *sender_y7_id == alice_id.to_uri() && text == "live")
    })
    .await?;

    // Bob goes offline.
    bob.shutdown().await?;
    drop(bob);
    sleep(Duration::from_secs(3)).await;

    // Alice sends 3 offline messages → queue.
    let offline_texts: Vec<String> = (0..3).map(|i| format!("offline {i}")).collect();
    for t in &offline_texts {
        alice.send_message(bob_id, t.clone()).await?;
    }

    // The bg push_one has a 5s SEND_TIMEOUT before it gives up + enqueues.
    // Poll for up to 30s on the queue state to handle slower CI runners.
    let mut cleared = 0;
    for _ in 0..60 {
        sleep(Duration::from_millis(500)).await;
        cleared = alice.debug_clear_outbound_queue(&bob_id).await?;
        if cleared >= 3 {
            break;
        }
    }
    assert!(
        cleared >= 3,
        "expected at least 3 queue entries to wipe, got {cleared}"
    );

    // Bob reboots with the same data dir → preserves identity, session, etc.
    let bob = AppHandle::boot(AppConfig::in_dir(bob_dir.path())).await?;
    let mut bob_events = bob.subscribe();
    sleep(Duration::from_secs(3)).await;

    // Wait for the 3 offline messages via sync.
    let mut got = std::collections::HashSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while got.len() < 3 && tokio::time::Instant::now() < deadline {
        let remaining = deadline.duration_since(tokio::time::Instant::now());
        match timeout(remaining, bob_events.recv()).await {
            Ok(Ok(AppEvent::MessageReceived {
                sender_y7_id, text, ..
            })) if sender_y7_id == alice_id.to_uri() && offline_texts.contains(&text) => {
                got.insert(text);
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) | Err(_) => break,
        }
    }
    assert_eq!(
        got.len(),
        3,
        "expected 3 sync-reconciled messages, got {got:?}"
    );

    // Bob's messages table should have 4 from Alice (1 live + 3 sync).
    let bob_messages = bob.list_messages(alice_id, 1000).await?;
    let from_alice: Vec<_> = bob_messages
        .into_iter()
        .filter(|m| m.sender_y7_id == alice_id.to_uri())
        .collect();
    assert_eq!(
        from_alice.len(),
        4,
        "expected 4 messages from alice, got {}",
        from_alice.len()
    );

    Ok(())
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
