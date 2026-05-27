//! V1 delete-propagation: Bob deletes the chat → Alice receives the
//! ContactRemoved event and her contact + messages are wiped.

use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

use y7ke_app::{AppConfig, AppHandle};
use y7ke_core::AppEvent;

const OVERALL_TIMEOUT: Duration = Duration::from_secs(60);
const MDNS_BUDGET: Duration = Duration::from_secs(20);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_propagates_to_peer() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("y7ke=info")
        .with_test_writer()
        .try_init();
    match timeout(OVERALL_TIMEOUT, scenario()).await {
        Err(_) => panic!("delete_propagates_to_peer timed out"),
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

    let mut bob_events = bob.subscribe();
    let mut alice_events = alice.subscribe();
    timeout(
        MDNS_BUDGET,
        retry_until_ok(|| alice.send_contact_request(bob_id, Some("hi".into()))),
    )
    .await??;
    wait_for_event(&mut bob_events, |ev| {
        matches!(ev, AppEvent::RequestReceived { y7_id, .. } if *y7_id == alice_id.to_uri())
    })
    .await?;
    let pending = bob.list_pending_requests().await?;
    let req = pending
        .into_iter()
        .find(|r| r.peer_y7_id == alice_id.to_uri())
        .ok_or("no pending on bob")?;
    bob.accept_request(req.id).await?;

    // Alice should receive the AcceptedRequest control → contact promoted.
    wait_for_event(&mut alice_events, |ev| {
        matches!(ev, AppEvent::ContactAdded { y7_id, .. } if *y7_id == bob_id.to_uri())
    })
    .await?;

    // Bob deletes the chat. Alice should see ContactRemoved.
    bob.delete_contact(alice_id).await?;
    wait_for_event(&mut alice_events, |ev| {
        matches!(ev, AppEvent::ContactRemoved { y7_id } if *y7_id == bob_id.to_uri())
    })
    .await?;

    // Alice's local state is wiped.
    let alice_contacts = alice.list_contacts().await?;
    assert!(
        !alice_contacts.iter().any(|c| c.y7_id == bob_id.to_uri()),
        "alice's contact list still has bob after remote delete: {alice_contacts:?}"
    );

    // Bob's local state is wiped.
    let bob_contacts = bob.list_contacts().await?;
    assert!(
        !bob_contacts.iter().any(|c| c.y7_id == alice_id.to_uri()),
        "bob's contact list still has alice after his own delete"
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
