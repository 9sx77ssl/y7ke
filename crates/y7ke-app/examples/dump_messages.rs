//! Ad-hoc debug example: opens Alice's on-disk DB and runs the exact decrypt
//! loop from `AppHandle::list_messages` against it, plus dumps contacts and
//! sessions. Used to verify whether the backend returns the persisted
//! messages for a given peer (debugging an empty-UI bug).
//!
//! Run:
//!   pkill -f '/target/release/y7ke'
//!   cargo run -p y7ke-app --example dump_messages -- <y7-peer-id>
//!
//! Env:
//!   Y7KE_DATA_DIR — defaults to `/tmp/y7ke-alice/y7ke` (the path the running
//!   binary writes to when launched with `XDG_DATA_HOME=/tmp/y7ke-alice`).

use std::env;

use y7ke_app::messaging;
use y7ke_core::crypto::VerifyingKey;
use y7ke_core::{ConversationId, Y7Id};
use y7ke_storage::{Db, DbConfig};

const DEFAULT_PEER: &str = "y7:7GerbVBAQzNWc3YzYJTpozQ2FckZJXmSZqu12aYHiUnY";
const DEFAULT_DATA_DIR: &str = "/tmp/y7ke-alice/y7ke";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn,y7ke=info")
        .with_writer(std::io::stderr)
        .try_init();

    let peer_uri = env::args().nth(1).unwrap_or_else(|| DEFAULT_PEER.into());
    let data_dir = env::var("Y7KE_DATA_DIR").unwrap_or_else(|_| DEFAULT_DATA_DIR.into());
    println!("data_dir = {data_dir}");
    println!("peer     = {peer_uri}");

    let peer = Y7Id::parse(&peer_uri)?;
    println!("peer pub = {}", hex_upper(peer.pubkey()));

    // Open the DB the same way the running binary does.
    let db = Db::open(DbConfig::in_dir(&data_dir)).await?;

    // Load local identity (decrypts the Ed25519 private with the DEK).
    let user = db
        .users()
        .get()
        .await?
        .ok_or("no local user row — wrong data_dir?")?;
    let signing_key = y7ke_core::crypto::SigningKey::from_bytes(&user.ed25519_priv);
    let my_pubkey = signing_key.verifying_key().to_bytes();
    let my_y7 = user.y7_id;
    println!("me       = {my_y7}");
    println!("me pub   = {}", hex_upper(&my_pubkey));

    // Compute the conv id the same way AppHandle does.
    let conv = ConversationId::between(&my_y7, &peer);
    println!("conv_id  = {} (uppercase: {})", conv.to_hex(), conv.to_hex().to_uppercase());

    // Sessions DAO sanity check.
    match db.sessions().get(&peer).await? {
        Some(s) => println!(
            "session  = present (established_at={}, last_used_at={})",
            s.established_at, s.last_used_at
        ),
        None => println!("session  = MISSING — list_messages would still work, but send_message wouldn't"),
    }

    // Contacts dump.
    let contacts = db.contacts().list().await?;
    println!("\ncontacts ({}):", contacts.len());
    for c in &contacts {
        println!(
            "  - {} status={:?} nickname={:?} pub={}",
            c.y7_id,
            c.status,
            c.nickname,
            hex_upper(&c.ed25519_pub),
        );
    }

    // Replicate AppHandle::list_messages exactly.
    let rows = db.messages().list_for_conversation(&conv, 500).await?;
    println!(
        "\nraw rows for conv {}: {} (querying with same bytes the prod code uses)",
        conv.to_hex().to_uppercase(),
        rows.len()
    );

    let conv_key = messaging::derive_conv_key(&signing_key, peer.pubkey(), conv.as_bytes())?;

    let mut text_count = 0;
    let mut control_count = 0;
    let mut fail_count = 0;
    let mut returned = 0;

    println!("\n--- decrypted messages (the list_messages return) ---");
    for (idx, m) in rows.iter().enumerate() {
        let sender_y7 = Y7Id::from_pubkey(m.sender_pub);
        let verifying = match VerifyingKey::from_bytes(&m.sender_pub) {
            Ok(v) => v,
            Err(e) => {
                fail_count += 1;
                println!(
                    "  [{idx:02}] BAD sender_pub {}: {e}",
                    hex_upper(&m.sender_pub)
                );
                continue;
            }
        };
        let envelope = y7ke_net::protocol::MessageEnvelope {
            message_id: *m.message_id.as_bytes(),
            sender_pub: m.sender_pub,
            timestamp_ms: m.timestamp_ms,
            nonce: m.payload_nonce,
            ciphertext: m.payload_enc.clone(),
            sig: m.sig,
        };
        let text = match messaging::open_envelope(&envelope, &verifying, &conv_key) {
            Ok(messaging::PlaintextKind::Text(t)) => {
                text_count += 1;
                returned += 1;
                t
            }
            Ok(messaging::PlaintextKind::Control(c)) => {
                control_count += 1;
                println!(
                    "  [{idx:02}] CONTROL (skipped) ts={} sender={} payload={:?}",
                    m.timestamp_ms, sender_y7, c
                );
                continue;
            }
            Err(e) => {
                fail_count += 1;
                returned += 1;
                println!(
                    "  [{idx:02}] DECRYPT FAIL ts={} sender={} status={} err={e}",
                    m.timestamp_ms,
                    sender_y7,
                    m.status.as_i64()
                );
                continue;
            }
        };
        let is_mine = m.sender_pub == my_pubkey;
        println!(
            "  [{idx:02}] mid={} ts={} sender={} is_mine={} status={} text={:?}",
            m.message_id,
            m.timestamp_ms,
            sender_y7.to_uri(),
            is_mine,
            m.status.as_i64(),
            text,
        );
    }

    println!(
        "\nSUMMARY: db_rows={} returned_by_list_messages={} (text={} fail={} control_skipped={})",
        rows.len(),
        returned,
        text_count,
        fail_count,
        control_count
    );

    Ok(())
}

fn hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}
