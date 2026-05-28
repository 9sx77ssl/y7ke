//! Per-peer reconnect throttling for the presence ticker's Offline arm.
//!
//! The ticker walks every Accepted contact each cycle. Without a guard,
//! every Offline peer is re-dialed on every tick forever — a steady
//! storm for contacts that have genuinely gone away (uninstalled,
//! powered off). [`Backoff`] bounds that: each peer gets an exponential
//! cooldown that grows with consecutive failed reconnect attempts and is
//! reset the moment a connection is established (see
//! `event_loop::ConnectionEstablished`).
//!
//! The type is pure and deterministic given an explicit `now` + jitter,
//! so the schedule is unit-testable without sleeping. It is distinct
//! from `AppInner::upgrade_backoff` (tick-count based, for the Relayed
//! "relay is temporary" arm) on purpose: that one throttles upgrades on
//! a *live* connection, this one throttles reconnects to a *dead* peer.

use std::time::{Duration, Instant};

/// Base cooldown — also the ticker's own interval, so the first retry
/// keeps the existing once-per-tick cadence and only backs off from
/// there.
const BASE: Duration = Duration::from_secs(30);
/// Step at which exponential growth saturates (2^6 = 64× base before the
/// ceiling clamps it).
const MAX_STEP: u32 = 6;
/// Hard ceiling on a single cooldown window. A permanently-offline
/// contact settles to one dial attempt every 10 minutes.
const CEILING: Duration = Duration::from_secs(600);

/// Exponential-backoff schedule for one peer's offline reconnect attempts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Backoff {
    attempts: u32,
    next_at: Instant,
}

impl Backoff {
    /// A fresh backoff whose first attempt is immediately due.
    pub(crate) fn ready(now: Instant) -> Self {
        Self {
            attempts: 0,
            next_at: now,
        }
    }

    /// Whether a reconnect attempt is permitted at `now`.
    pub(crate) fn due(&self, now: Instant) -> bool {
        now >= self.next_at
    }

    /// Record that an attempt fired at `now`; schedule the next one after
    /// the current step's cooldown plus `jitter` (desyncs peers that all
    /// went offline together so they don't retry on the same tick).
    pub(crate) fn record(&mut self, now: Instant, jitter: Duration) {
        let wait = cooldown_for(self.attempts).saturating_add(jitter);
        self.attempts = self.attempts.saturating_add(1);
        self.next_at = now + wait;
    }
}

/// Cooldown for a given attempt step: `BASE * 2^min(step, MAX_STEP)`,
/// clamped to `CEILING`.
fn cooldown_for(step: u32) -> Duration {
    let factor = 1u32 << step.min(MAX_STEP);
    BASE.checked_mul(factor).unwrap_or(CEILING).min(CEILING)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_is_immediately_due() {
        let now = Instant::now();
        let b = Backoff::ready(now);
        assert!(b.due(now));
    }

    #[test]
    fn record_pushes_next_attempt_out_by_base_then_grows() {
        let now = Instant::now();
        let mut b = Backoff::ready(now);

        // step 0 cooldown = BASE
        b.record(now, Duration::ZERO);
        assert!(!b.due(now + Duration::from_secs(29)));
        assert!(b.due(now + Duration::from_secs(30)));

        // step 1 cooldown = 2 * BASE
        let t1 = now + Duration::from_secs(30);
        b.record(t1, Duration::ZERO);
        assert!(!b.due(t1 + Duration::from_secs(59)));
        assert!(b.due(t1 + Duration::from_secs(60)));
    }

    #[test]
    fn cooldown_saturates_at_ceiling() {
        assert_eq!(cooldown_for(0), BASE);
        assert_eq!(cooldown_for(1), BASE * 2);
        assert_eq!(cooldown_for(4), BASE * 16);
        // step 5 → 30s * 32 = 960s, clamped to the 600s ceiling.
        assert_eq!(cooldown_for(5), CEILING);
        assert_eq!(cooldown_for(99), CEILING);
    }

    #[test]
    fn jitter_extends_the_cooldown() {
        let now = Instant::now();
        let mut b = Backoff::ready(now);
        b.record(now, Duration::from_millis(500));
        // base 30s alone is no longer enough; need the +500ms too.
        assert!(!b.due(now + Duration::from_secs(30)));
        assert!(b.due(now + Duration::from_millis(30_500)));
    }
}
