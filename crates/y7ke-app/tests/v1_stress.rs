//! V1 stress check: three concurrent clients exchanging messages over a
//! single mDNS swarm. Useful both as a smoke test for V1 release polish and
//! as a regression guard against synchronization issues in the event loop.
//!
//! Marked `#[ignore]` by default — it takes ~30s and is meant to be invoked
//! explicitly via `cargo test -p y7ke-app --test v1_stress -- --ignored`.

// Indexed pair-iteration is the clearest shape for the (i, j) sender/receiver
// scenario; using `enumerate` here would obscure intent.
#![allow(clippy::needless_range_loop)]

use std::collections::HashSet;
use std::time::Duration;

use tempfile::TempDir;
use tokio::time::{sleep, timeout};

use y7ke_app::{AppConfig, AppHandle};
use y7ke_core::Y7Id;

const OVERALL_TIMEOUT: Duration = Duration::from_secs(120);
const MDNS_BUDGET: Duration = Duration::from_secs(20);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "stress test — invoke with --ignored"]
async fn three_clients_pairwise_message_exchange() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=info")
        .with_test_writer()
        .try_init();

    let result = timeout(OVERALL_TIMEOUT, scenario()).await;
    match result {
        Err(_) => panic!("stress test timed out after {OVERALL_TIMEOUT:?}"),
        Ok(Err(e)) => panic!("stress test failed: {e}"),
        Ok(Ok(())) => {}
    }
}

async fn scenario() -> Result<(), Box<dyn std::error::Error>> {
    const MSGS_PER_DIRECTION: usize = 5;

    let dirs: Vec<TempDir> = (0..3).map(|_| TempDir::new().unwrap()).collect();
    let clients: Vec<AppHandle> = {
        let mut v = Vec::new();
        for d in &dirs {
            v.push(AppHandle::boot(AppConfig::in_dir(d.path())).await?);
        }
        v
    };
    let ids: Vec<Y7Id> = clients.iter().map(|c| *c.my_y7_id()).collect();

    sleep(Duration::from_secs(3)).await;

    // Pairwise handshakes: client[i] requests client[j] for i<j (3 pairs).
    for i in 0..clients.len() {
        for j in 0..clients.len() {
            if i == j {
                continue;
            }
            // Each side sends a request to the other so both have a session
            // for the other. (Symmetric handshake — order doesn't matter.)
            let _ = timeout(
                MDNS_BUDGET,
                retry_until_ok(|| {
                    clients[i].send_contact_request(ids[j], Some(format!("from {i} to {j}")))
                }),
            )
            .await?;
        }
    }

    sleep(Duration::from_secs(2)).await;

    // Auto-accept all pending requests on every client.
    for c in &clients {
        let pending = c.list_pending_requests().await?;
        for r in pending {
            c.accept_request(r.id).await?;
        }
    }

    // Pairwise message exchange. Every client sends MSGS_PER_DIRECTION to
    // each peer.
    let mut sent_per_direction: std::collections::HashMap<(usize, usize), Vec<String>> =
        Default::default();
    for i in 0..clients.len() {
        for j in 0..clients.len() {
            if i == j {
                continue;
            }
            let mut texts = Vec::new();
            for k in 0..MSGS_PER_DIRECTION {
                let text = format!("msg from {i} to {j} #{k}");
                clients[i].send_message(ids[j], text.clone()).await?;
                texts.push(text);
            }
            sent_per_direction.insert((i, j), texts);
        }
    }

    // Allow some time for everything to propagate.
    sleep(Duration::from_secs(5)).await;

    // Verify every client has every message addressed to them.
    let n = clients.len();
    for j in 0..n {
        for i in 0..n {
            if i == j {
                continue;
            }
            let expected: HashSet<String> = sent_per_direction[&(i, j)].iter().cloned().collect();
            let got = clients[j].list_messages(ids[i], 1000).await?;
            let got_texts: HashSet<String> = got
                .iter()
                .filter(|m| !m.is_mine)
                .map(|m| m.text.clone())
                .collect();
            let missing: Vec<&String> = expected.difference(&got_texts).collect();
            assert!(
                missing.is_empty(),
                "client {j} missing {} messages from {i}: {:?}",
                missing.len(),
                missing,
            );
        }
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
