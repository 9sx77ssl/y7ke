//! End-to-end exercise of the storage layer: open Db, persist identity with
//! encrypted private key, list/insert across all the other tables, close
//! and reopen to verify durability.

use tempfile::TempDir;
use y7ke_core::crypto::SigningKey;
use y7ke_core::{ContactStatus, ConversationId, MessageId, MessageStatus, Y7Id};
use y7ke_storage::dao::contacts::NewContact;
use y7ke_storage::dao::messages::NewMessage;
use y7ke_storage::dao::requests::{NewRequest, RequestDirection};
use y7ke_storage::dao::users::NewUser;
use y7ke_storage::{Db, DbConfig};

#[tokio::test]
async fn users_round_trip_with_encrypted_private_key() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

    let signing = SigningKey::generate();
    let pub_bytes = signing.verifying_key().to_bytes();
    let priv_bytes = signing.to_bytes();
    let y7_id = Y7Id::from_pubkey(pub_bytes);

    let user = db
        .users()
        .insert(NewUser {
            y7_id,
            ed25519_pub: pub_bytes,
            ed25519_priv: priv_bytes,
        })
        .await
        .unwrap();

    assert_eq!(user.ed25519_priv, priv_bytes);

    // Reload from disk to confirm we don't depend on in-memory state.
    drop(db);
    let db2 = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();
    let loaded = db2.users().get().await.unwrap().expect("user should exist");
    assert_eq!(loaded.y7_id, y7_id);
    assert_eq!(loaded.ed25519_priv, priv_bytes);

    // Confirm the on-disk ciphertext is NOT the plaintext.
    let raw: (Vec<u8>,) = sqlx::query_as("SELECT ed25519_priv_enc FROM users WHERE id = 1")
        .fetch_one(db2.pool())
        .await
        .unwrap();
    assert_ne!(&raw.0[..32], &priv_bytes[..]);
}

#[tokio::test]
async fn contacts_insert_list_update_status() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

    let y7 = Y7Id::from_pubkey([9u8; 32]);
    db.contacts()
        .insert(NewContact {
            y7_id: y7,
            ed25519_pub: [9u8; 32],
            nickname: Some("alice".into()),
            status: ContactStatus::PendingOut,
        })
        .await
        .unwrap();

    let list = db.contacts().list().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].nickname.as_deref(), Some("alice"));
    assert_eq!(list[0].status, ContactStatus::PendingOut);

    db.contacts()
        .update_status(&y7, ContactStatus::Accepted)
        .await
        .unwrap();

    let again = db.contacts().get(&y7).await.unwrap().unwrap();
    assert_eq!(again.status, ContactStatus::Accepted);
}

#[tokio::test]
async fn requests_encrypts_greeting_and_round_trips() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

    let peer = Y7Id::from_pubkey([3u8; 32]);
    db.requests()
        .insert(NewRequest {
            direction: RequestDirection::Incoming,
            peer_y7_id: peer,
            initial_text: Some("hi there".into()),
        })
        .await
        .unwrap();

    let pending = db.requests().list_pending(None).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].initial_text.as_deref(), Some("hi there"));
}

#[tokio::test]
async fn messages_insert_or_ignore_dedups() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

    let a = Y7Id::from_pubkey([1u8; 32]);
    let b = Y7Id::from_pubkey([2u8; 32]);
    let conv = ConversationId::between(&a, &b);
    let mid = MessageId::new_v7();

    let new = NewMessage {
        message_id: mid,
        conversation_id: conv,
        sender_pub: [1u8; 32],
        recipient_pub: [2u8; 32],
        timestamp_ms: 1_700_000_000_000,
        status: MessageStatus::Sending,
        payload_enc: vec![0xab; 32],
        payload_nonce: [7u8; 12],
        sig: [0xff; 64],
    };
    assert!(db.messages().insert(new).await.unwrap()); // first insert succeeds

    // Same message ID inserted again must be a no-op (no duplicate row).
    let dup = NewMessage {
        message_id: mid,
        conversation_id: conv,
        sender_pub: [1u8; 32],
        recipient_pub: [2u8; 32],
        timestamp_ms: 1_700_000_000_001,
        status: MessageStatus::Sent,
        payload_enc: vec![0xcd; 16],
        payload_nonce: [8u8; 12],
        sig: [0xee; 64],
    };
    assert!(!db.messages().insert(dup).await.unwrap());

    let listed = db
        .messages()
        .list_for_conversation(&conv, 50)
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].status, MessageStatus::Sending);

    db.messages()
        .update_status(&mid, MessageStatus::Synced)
        .await
        .unwrap();
    let again = db
        .messages()
        .list_for_conversation(&conv, 50)
        .await
        .unwrap();
    assert_eq!(again[0].status, MessageStatus::Synced);
}

#[tokio::test]
async fn sessions_round_trip() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

    let peer = Y7Id::from_pubkey([4u8; 32]);
    db.sessions().upsert(&peer).await.unwrap();

    let got = db.sessions().get(&peer).await.unwrap().unwrap();
    assert_eq!(got.peer_y7_id, peer);
    assert!(got.established_at > 0);

    // No session for an unknown peer.
    let other = Y7Id::from_pubkey([5u8; 32]);
    assert!(db.sessions().get(&other).await.unwrap().is_none());
}

#[tokio::test]
async fn sync_queue_enqueue_due_remove() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

    let target = Y7Id::from_pubkey([5u8; 32]);
    let mid = MessageId::new_v7();
    db.sync_queue().enqueue(&mid, &target, 1_000).await.unwrap();

    let due = db.sync_queue().due(2_000, 10).await.unwrap();
    assert_eq!(due.len(), 1);

    db.sync_queue()
        .bump(&mid, &target, 1, 10_000)
        .await
        .unwrap();
    let still_due = db.sync_queue().due(2_000, 10).await.unwrap();
    assert!(still_due.is_empty());

    db.sync_queue().remove(&mid, &target).await.unwrap();
    let after = db.sync_queue().due(i64::MAX, 10).await.unwrap();
    assert!(after.is_empty());
}

#[tokio::test]
async fn peer_state_upserts() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

    let peer = Y7Id::from_pubkey([6u8; 32]);
    db.peer_state()
        .upsert_seen(&peer, Some("[\"/ip4/127.0.0.1/tcp/40000\"]".into()))
        .await
        .unwrap();

    let mid = MessageId::new_v7();
    db.peer_state()
        .set_high_water_inbound(&peer, &mid)
        .await
        .unwrap();

    let state = db.peer_state().get(&peer).await.unwrap().unwrap();
    assert!(state.last_seen_at.is_some());
    assert_eq!(state.highest_seen_message_id, Some(mid));
}

#[tokio::test]
async fn pending_deletes_enqueue_due_bump_remove() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

    let peer = Y7Id::from_pubkey([7u8; 32]);
    let envelope = vec![0xab; 64];
    db.pending_deletes()
        .enqueue(&peer, &envelope, 1_000)
        .await
        .unwrap();

    let got = db.pending_deletes().get(&peer).await.unwrap().unwrap();
    assert_eq!(got.peer_y7_id, peer);
    assert_eq!(got.envelope, envelope);
    assert_eq!(got.attempts, 0);

    let due = db.pending_deletes().due(2_000, 10).await.unwrap();
    assert_eq!(due.len(), 1);

    // Bump past the window → no longer due.
    db.pending_deletes().bump(&peer, 1, 10_000).await.unwrap();
    assert!(db
        .pending_deletes()
        .due(2_000, 10)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        db.pending_deletes()
            .get(&peer)
            .await
            .unwrap()
            .unwrap()
            .attempts,
        1
    );

    db.pending_deletes().remove(&peer).await.unwrap();
    assert!(db.pending_deletes().get(&peer).await.unwrap().is_none());
}

/// The sealed ChatDeleted must outlive the local wipe — otherwise the delete
/// could never reach an offline peer. wipe_peer clears the session/contact/
/// messages but must leave the pending_deletes row intact for retry.
#[tokio::test]
async fn pending_delete_survives_wipe_peer() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

    let me = Y7Id::from_pubkey([1u8; 32]);
    let peer = Y7Id::from_pubkey([2u8; 32]);
    let conv = ConversationId::between(&me, &peer);

    // State that wipe_peer SHOULD clear.
    db.sessions().upsert(&peer).await.unwrap();
    db.contacts()
        .insert(NewContact {
            y7_id: peer,
            ed25519_pub: [2u8; 32],
            nickname: None,
            status: ContactStatus::Accepted,
        })
        .await
        .unwrap();

    // The durable delete envelope, queued before the wipe.
    db.pending_deletes()
        .enqueue(&peer, &[0xcd; 48], 0)
        .await
        .unwrap();

    db.wipe_peer(&peer, &conv).await.unwrap();

    assert!(db.sessions().get(&peer).await.unwrap().is_none());
    assert!(db.contacts().get(&peer).await.unwrap().is_none());
    // The deletion outbox survives so it can still be delivered on reconnect.
    assert!(db.pending_deletes().get(&peer).await.unwrap().is_some());
}
