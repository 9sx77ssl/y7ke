//! V1 privacy verification: prove that the plaintext of a sent message does
//! NOT appear anywhere in either client's on-disk database after a normal
//! send/receive cycle.
//!
//! This catches regressions where someone accidentally stores the plaintext
//! alongside the ciphertext, leaks it through a log buffer that hits disk,
//! or fails to encrypt one of the participant rows. With the V1 architecture
//! every plaintext stays in memory; the DB only ever sees `payload_enc`.

use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

use y7ke_app::{AppConfig, AppHandle};
use y7ke_core::AppEvent;

const OVERALL_TIMEOUT: Duration = Duration::from_secs(45);
const MDNS_BUDGET: Duration = Duration::from_secs(20);

const PLAINTEXT: &str = "PRIVACY_CANARY_alpha_bravo_charlie_42";

#[cfg_attr(
    any(target_os = "macos", target_os = "windows"),
    ignore = "mDNS unreliable on GitHub Actions runners"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn message_plaintext_never_lands_on_disk() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=info")
        .with_test_writer()
        .try_init();

    let result = timeout(OVERALL_TIMEOUT, scenario()).await;
    match result {
        Err(_) => panic!("privacy test timed out after {OVERALL_TIMEOUT:?}"),
        Ok(Err(e)) => panic!("privacy test failed: {e}"),
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
    let mut alice_events = alice.subscribe();
    timeout(
        MDNS_BUDGET,
        retry_until_ok(|| alice.send_contact_request(bob_id, Some("canary".into()))),
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
        .ok_or("bob missing request")?;
    bob.accept_request(req.id).await?;

    alice.send_message(bob_id, PLAINTEXT.into()).await?;
    wait_for_event(
        &mut bob_events,
        |ev| matches!(ev, AppEvent::MessageReceived { text, .. } if text == PLAINTEXT),
    )
    .await?;

    bob.send_message(alice_id, PLAINTEXT.into()).await?;
    wait_for_event(
        &mut alice_events,
        |ev| matches!(ev, AppEvent::MessageReceived { text, .. } if text == PLAINTEXT),
    )
    .await?;

    // Make sure both DBs have flushed (SQLite WAL is asynchronous).
    alice.shutdown().await?;
    bob.shutdown().await?;
    drop(alice);
    drop(bob);
    sleep(Duration::from_millis(500)).await;

    // Scan every byte of every persisted file in both data directories for
    // the plaintext canary. The DEK file is binary random; ciphertext is
    // random; nothing on disk should match the canary.
    let canary = PLAINTEXT.as_bytes();
    for (label, dir) in [("alice", alice_dir.path()), ("bob", bob_dir.path())] {
        for entry in walk(dir)? {
            let bytes = std::fs::read(&entry)?;
            if find_subslice(&bytes, canary).is_some() {
                return Err(
                    format!("{label}'s {} contains plaintext canary", entry.display()).into(),
                );
            }
        }
    }

    Ok(())
}

fn walk(root: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        for entry in std::fs::read_dir(&p)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
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
