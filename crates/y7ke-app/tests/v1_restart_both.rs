//! V1 acceptance: both clients restart with same data directories.
//!
//! Scenario: Alice and Bob handshake and exchange messages. Both shut down.
//! Both reboot using the same data directories. Identity, session, and chat
//! history all persist. The pair rediscovers via mDNS and a new message
//! flows in both directions.

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
async fn restart_both_clients_preserves_history_and_resumes() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=debug,info")
        .with_test_writer()
        .try_init();

    let result = timeout(OVERALL_TIMEOUT, scenario()).await;
    match result {
        Err(_) => panic!("restart_both timed out after {OVERALL_TIMEOUT:?}"),
        Ok(Err(e)) => panic!("restart_both failed: {e}"),
        Ok(Ok(())) => {}
    }
}

async fn scenario() -> Result<(), Box<dyn std::error::Error>> {
    let alice_dir = TempDir::new()?;
    let bob_dir = TempDir::new()?;

    // First boot.
    let alice = AppHandle::boot(AppConfig::in_dir(alice_dir.path())).await?;
    let bob = AppHandle::boot(AppConfig::in_dir(bob_dir.path())).await?;
    let alice_id = *alice.my_y7_id();
    let bob_id = *bob.my_y7_id();

    sleep(Duration::from_secs(3)).await;

    let mut bob_events = bob.subscribe();
    timeout(
        MDNS_BUDGET,
        retry_until_ok(|| alice.send_contact_request(bob_id, Some("hi".into()))),
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
        .ok_or("no pending")?;
    bob.accept_request(req.id).await?;

    // Exchange initial messages.
    let mut alice_events = alice.subscribe();
    let mut bob_events = bob.subscribe();
    alice
        .send_message(bob_id, "before restart from alice".into())
        .await?;
    wait_for_event(&mut bob_events, |ev| {
        matches!(ev, AppEvent::MessageReceived { text, .. } if text == "before restart from alice")
    })
    .await?;
    bob.send_message(alice_id, "before restart from bob".into())
        .await?;
    wait_for_event(&mut alice_events, |ev| {
        matches!(ev, AppEvent::MessageReceived { text, .. } if text == "before restart from bob")
    })
    .await?;

    let alice_before = alice.list_messages(bob_id, 100).await?;
    let bob_before = bob.list_messages(alice_id, 100).await?;
    assert_eq!(
        alice_before.len(),
        2,
        "alice should have 2 messages before restart"
    );
    assert_eq!(
        bob_before.len(),
        2,
        "bob should have 2 messages before restart"
    );

    // Both shut down (graceful).
    alice.shutdown().await?;
    bob.shutdown().await?;
    drop(alice);
    drop(bob);
    sleep(Duration::from_secs(2)).await;

    // Reboot both with the same data directories.
    let alice2 = AppHandle::boot(AppConfig::in_dir(alice_dir.path())).await?;
    let bob2 = AppHandle::boot(AppConfig::in_dir(bob_dir.path())).await?;

    // Identity must persist.
    assert_eq!(*alice2.my_y7_id(), alice_id);
    assert_eq!(*bob2.my_y7_id(), bob_id);

    // History must persist.
    let alice_after_boot = alice2.list_messages(bob_id, 100).await?;
    let bob_after_boot = bob2.list_messages(alice_id, 100).await?;
    assert_eq!(
        alice_after_boot.len(),
        2,
        "alice's history must survive restart"
    );
    assert_eq!(
        bob_after_boot.len(),
        2,
        "bob's history must survive restart"
    );

    // Decrypted text must match too.
    assert!(alice_after_boot
        .iter()
        .any(|m| m.text == "before restart from bob"));
    assert!(bob_after_boot
        .iter()
        .any(|m| m.text == "before restart from alice"));

    // Resume: send a new message after reboot.
    sleep(Duration::from_secs(3)).await;
    let mut bob_events = bob2.subscribe();
    alice2
        .send_message(bob_id, "after restart from alice".into())
        .await?;
    wait_for_event(&mut bob_events, |ev| {
        matches!(ev, AppEvent::MessageReceived { text, .. } if text == "after restart from alice")
    })
    .await?;

    let alice_final = alice2.list_messages(bob_id, 100).await?;
    let bob_final = bob2.list_messages(alice_id, 100).await?;
    assert_eq!(alice_final.len(), 3, "alice should now have 3 messages");
    assert_eq!(bob_final.len(), 3, "bob should now have 3 messages");

    // H3 regression: a single inbound request must not produce duplicate rows
    // if the initiator retries while a session already exists.
    //
    // After the H1 fix, alice2.send_contact_request(bob_id, ...) is a no-op
    // because session already exists. So bob2 should still see exactly 1
    // (resolved) request and 0 pending. Confirm.
    alice2
        .send_contact_request(bob_id, Some("duplicate request".into()))
        .await?;
    sleep(Duration::from_secs(1)).await;
    let bob_pending_after = bob2.list_pending_requests().await?;
    assert!(
        bob_pending_after.is_empty(),
        "no new pending requests should appear; got {:?}",
        bob_pending_after
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
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
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
