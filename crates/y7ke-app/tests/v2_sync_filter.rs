//! V2-C: SyncReq::Pull responder must only stream rows it itself signed.
//!
//! Regression for the "synced envelope signed by wrong key" warning:
//! before the filter landed, a Pull responder echoed back the entire
//! conversation (both directions) which made the requester's
//! `ingest_synced_envelope` reject half the payload. This test exercises
//! the round trip and asserts both peers end up with the expected
//! message count and no duplicates after a restart-driven sync.

use std::collections::HashSet;
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
async fn sync_filter_no_cross_signed_duplicates() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=debug,info")
        .with_test_writer()
        .try_init();
    match timeout(OVERALL_TIMEOUT, scenario()).await {
        Err(_) => panic!("sync_filter test timed out"),
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
    let mut alice_events = alice.subscribe();
    let mut bob_events = bob.subscribe();
    timeout(
        MDNS_BUDGET,
        retry_until_ok(|| alice.send_contact_request(bob_id, Some("sync filter".into()))),
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

    // Bidirectional live exchange: 2 from Alice, 2 from Bob, so the
    // shared conversation contains rows signed by BOTH peers. The
    // filter has to suppress each side's foreign-signed rows during
    // sync, otherwise duplicates land on restart.
    let alice_texts = ["a-live-0", "a-live-1"];
    let bob_texts = ["b-live-0", "b-live-1"];

    for t in alice_texts {
        alice.send_message(bob_id, t.into()).await?;
    }
    for _ in 0..alice_texts.len() {
        wait_for_event(&mut bob_events, |ev| {
            matches!(ev, AppEvent::MessageReceived { sender_y7_id, .. }
                if *sender_y7_id == alice_id.to_uri())
        })
        .await?;
    }

    for t in bob_texts {
        bob.send_message(alice_id, t.into()).await?;
    }
    for _ in 0..bob_texts.len() {
        wait_for_event(&mut alice_events, |ev| {
            matches!(ev, AppEvent::MessageReceived { sender_y7_id, .. }
                if *sender_y7_id == bob_id.to_uri())
        })
        .await?;
    }

    // Restart Bob → he comes back online and kicks sync against Alice
    // (whose pull responder is the code under test). If the responder
    // echoed Bob's own envelopes back, ingest would log "signed by
    // wrong key" and we'd see double-counting.
    bob.shutdown().await?;
    drop(bob);
    sleep(Duration::from_secs(3)).await;

    let bob = AppHandle::boot(AppConfig::in_dir(bob_dir.path())).await?;
    sleep(Duration::from_secs(6)).await;

    // Each side stores 4 distinct messages (2 inbound + 2 outbound).
    // The filter ensures sync doesn't add duplicates.
    let alice_msgs = alice.list_messages(bob_id, 1000).await?;
    let bob_msgs = bob.list_messages(alice_id, 1000).await?;

    let alice_texts_set: HashSet<&str> = alice_msgs.iter().map(|m| m.text.as_str()).collect();
    let bob_texts_set: HashSet<&str> = bob_msgs.iter().map(|m| m.text.as_str()).collect();

    let want: HashSet<&str> = alice_texts
        .iter()
        .copied()
        .chain(bob_texts.iter().copied())
        .collect();
    assert_eq!(
        alice_texts_set, want,
        "alice should hold exactly the 4 distinct messages, got {alice_texts_set:?}"
    );
    assert_eq!(
        bob_texts_set, want,
        "bob should hold exactly the 4 distinct messages, got {bob_texts_set:?}"
    );

    // Distinct count == row count → no duplicates inserted by sync.
    assert_eq!(
        alice_msgs.len(),
        4,
        "alice has duplicate rows: {} messages, expected 4",
        alice_msgs.len()
    );
    assert_eq!(
        bob_msgs.len(),
        4,
        "bob has duplicate rows: {} messages, expected 4",
        bob_msgs.len()
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
