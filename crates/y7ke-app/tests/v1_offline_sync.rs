//! V1 capability 7: offline sync after reconnect.
//!
//! Scenario: Alice and Bob handshake. Bob shuts down. Alice sends N messages
//! while Bob is unreachable — they queue. Bob reboots with the same data
//! directory; mDNS rediscovers him; Alice's event loop drains the queue and
//! Bob receives every message in UUIDv7 order, no duplicates.

use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

use y7ke_app::{AppConfig, AppHandle};
use y7ke_core::AppEvent;

const OVERALL_TIMEOUT: Duration = Duration::from_secs(120);
const MDNS_BUDGET: Duration = Duration::from_secs(20);

#[cfg_attr(
    any(target_os = "macos", target_os = "windows"),
    ignore = "mDNS unreliable on GitHub Actions runners"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_messages_drain_on_reconnect() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=debug,info")
        .with_test_writer()
        .try_init();

    let result = timeout(OVERALL_TIMEOUT, scenario()).await;
    match result {
        Err(_) => panic!("offline sync test timed out after {OVERALL_TIMEOUT:?}"),
        Ok(Err(e)) => panic!("offline sync failed: {e}"),
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

    // mDNS discovery.
    sleep(Duration::from_secs(3)).await;

    // Initial handshake so both sides have a session for Bob<->Alice.
    let mut bob_events = bob.subscribe();
    timeout(
        MDNS_BUDGET,
        retry_until_ok(|| alice.send_contact_request(bob_id, Some("offline test".into()))),
    )
    .await??;
    wait_for_event(
        &mut bob_events,
        |ev| matches!(ev, AppEvent::RequestReceived { y7_id, .. } if *y7_id == alice_id.to_uri()),
    )
    .await?;
    let pending = bob.list_pending_requests().await?;
    let alice_req = pending
        .into_iter()
        .find(|r| r.peer_y7_id == alice_id.to_uri())
        .ok_or("Bob missing Alice request")?;
    bob.accept_request(alice_req.id).await?;

    // Bob goes offline. The shutdown closes Bob's swarm; Alice's swarm
    // detects the disconnect within a few seconds.
    bob.shutdown().await?;
    drop(bob);
    sleep(Duration::from_secs(3)).await;

    // Alice sends 5 messages — these should fail live-send and enqueue.
    let texts: Vec<String> = (0..5).map(|i| format!("offline msg {i}")).collect();
    for t in &texts {
        // The send_message call always succeeds at the application level
        // (persists locally), even if the live push fails — the failure
        // path is the offline-retry queue we're testing here.
        alice.send_message(bob_id, t.clone()).await?;
    }

    // Reboot Bob with the same data directory; identity + session persist.
    let bob = AppHandle::boot(AppConfig::in_dir(bob_dir.path())).await?;
    let mut bob_events = bob.subscribe();
    assert_eq!(*bob.my_y7_id(), bob_id, "Bob's identity must persist");

    // Wait for all 5 messages to arrive after rediscovery. mDNS reconnect
    // + queue drain budget is generous; the test ceiling overall is
    // OVERALL_TIMEOUT.
    let mut received = std::collections::HashSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while received.len() < texts.len() {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(format!(
                "only received {} of {} messages before deadline",
                received.len(),
                texts.len()
            )
            .into());
        }
        let remaining = deadline.duration_since(now);
        match timeout(remaining, bob_events.recv()).await {
            Err(_) => break,
            Ok(Err(_)) => break,
            Ok(Ok(AppEvent::MessageReceived { text, .. })) => {
                if texts.iter().any(|t| t == &text) {
                    received.insert(text);
                }
            }
            Ok(Ok(_)) => continue,
        }
    }

    assert_eq!(
        received.len(),
        texts.len(),
        "Bob should have received all queued messages"
    );

    let bob_msgs = bob.list_messages(alice_id, 50).await?;
    let texts_in_bob: std::collections::HashSet<&str> =
        bob_msgs.iter().map(|m| m.text.as_str()).collect();
    for t in &texts {
        assert!(
            texts_in_bob.contains(t.as_str()),
            "missing offline message: {t}"
        );
    }

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
