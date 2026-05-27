//! Per-peer leaky-bucket rate limiter for inbound RPCs.
//!
//! Three independent buckets per peer: handshake, msg, sync.
//! Each bucket is a sliding count of events in the trailing window.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use libp2p::PeerId;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct BucketLimit {
    pub max_events: usize,
    pub window: Duration,
}

#[derive(Default)]
struct Bucket {
    events: Vec<Instant>,
}

impl Bucket {
    fn try_consume(&mut self, limit: BucketLimit, now: Instant) -> bool {
        let cutoff = now - limit.window;
        self.events.retain(|t| *t >= cutoff);
        if self.events.len() >= limit.max_events {
            false
        } else {
            self.events.push(now);
            true
        }
    }
}

#[derive(Default)]
struct PeerBuckets {
    handshake: Bucket,
    msg: Bucket,
    sync: Bucket,
}

pub struct RateLimiter {
    inner: Mutex<HashMap<PeerId, PeerBuckets>>,
    pub handshake_limit: BucketLimit,
    pub msg_limit: BucketLimit,
    pub sync_limit: BucketLimit,
}

impl RateLimiter {
    pub fn default_limits() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            handshake_limit: BucketLimit {
                max_events: 30,
                window: Duration::from_secs(60),
            },
            msg_limit: BucketLimit {
                max_events: 600,
                window: Duration::from_secs(60),
            },
            sync_limit: BucketLimit {
                max_events: 30,
                window: Duration::from_secs(60),
            },
        }
    }

    pub async fn allow_handshake(&self, peer: PeerId) -> bool {
        let limit = self.handshake_limit;
        let mut g = self.inner.lock().await;
        let entry = g.entry(peer).or_default();
        entry.handshake.try_consume(limit, Instant::now())
    }

    pub async fn allow_msg(&self, peer: PeerId) -> bool {
        let limit = self.msg_limit;
        let mut g = self.inner.lock().await;
        let entry = g.entry(peer).or_default();
        entry.msg.try_consume(limit, Instant::now())
    }

    pub async fn allow_sync(&self, peer: PeerId) -> bool {
        let limit = self.sync_limit;
        let mut g = self.inner.lock().await;
        let entry = g.entry(peer).or_default();
        entry.sync.try_consume(limit, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer() -> PeerId {
        libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id()
    }

    #[tokio::test]
    async fn allows_under_limit_blocks_over() {
        let rl = RateLimiter {
            inner: Mutex::new(HashMap::new()),
            handshake_limit: BucketLimit {
                max_events: 3,
                window: Duration::from_secs(60),
            },
            msg_limit: BucketLimit {
                max_events: 100,
                window: Duration::from_secs(60),
            },
            sync_limit: BucketLimit {
                max_events: 100,
                window: Duration::from_secs(60),
            },
        };
        let p = peer();
        assert!(rl.allow_handshake(p).await);
        assert!(rl.allow_handshake(p).await);
        assert!(rl.allow_handshake(p).await);
        assert!(!rl.allow_handshake(p).await);
    }

    #[tokio::test]
    async fn buckets_are_independent() {
        let rl = RateLimiter {
            inner: Mutex::new(HashMap::new()),
            handshake_limit: BucketLimit {
                max_events: 1,
                window: Duration::from_secs(60),
            },
            msg_limit: BucketLimit {
                max_events: 1,
                window: Duration::from_secs(60),
            },
            sync_limit: BucketLimit {
                max_events: 1,
                window: Duration::from_secs(60),
            },
        };
        let p = peer();
        assert!(rl.allow_handshake(p).await);
        assert!(!rl.allow_handshake(p).await);
        assert!(rl.allow_msg(p).await);
        assert!(rl.allow_sync(p).await);
    }

    #[tokio::test]
    async fn peers_are_independent() {
        let rl = RateLimiter {
            inner: Mutex::new(HashMap::new()),
            handshake_limit: BucketLimit {
                max_events: 1,
                window: Duration::from_secs(60),
            },
            msg_limit: BucketLimit {
                max_events: 1,
                window: Duration::from_secs(60),
            },
            sync_limit: BucketLimit {
                max_events: 1,
                window: Duration::from_secs(60),
            },
        };
        let a = peer();
        let b = peer();
        assert!(rl.allow_handshake(a).await);
        assert!(rl.allow_handshake(b).await);
    }
}
