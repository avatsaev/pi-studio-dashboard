//! Liveness pings: one every 15 s while live; two consecutive unanswered pings mean the
//! connection is dead. RTT uses the local monotonic clock only (daemon timestamps are not used).

use std::time::Duration;

use pistudiodashboard_protocol::Ping;
use tokio::time::Instant;

pub(crate) const PING_EVERY: Duration = Duration::from_secs(15);
const MAX_MISSED: u32 = 2;

pub(crate) enum Tick {
    /// Send this encoded ping.
    Send(String),
    /// Too many unanswered pings: close and back off.
    Dead,
}

#[derive(Default)]
pub(crate) struct Pinger {
    next_id: u64,
    pending: Option<(String, Instant)>,
    missed: u32,
}

impl Pinger {
    /// Called every [`PING_EVERY`]. `now_ms` is only the informational `clientSentAt` stamp.
    pub(crate) fn on_tick(&mut self, now: Instant, now_ms: i64) -> Tick {
        if self.pending.take().is_some() {
            self.missed += 1;
            if self.missed >= MAX_MISSED {
                return Tick::Dead;
            }
        }
        self.next_id += 1;
        let request_id = format!("rtt-{}", self.next_id);
        let encoded = Ping { request_id: request_id.clone(), client_sent_at: now_ms }.encode();
        self.pending = Some((request_id, now));
        Tick::Send(encoded)
    }

    /// A pong arrived; returns the RTT in ms when it answers the outstanding ping.
    pub(crate) fn on_pong(&mut self, request_id: &str, now: Instant) -> Option<u64> {
        let (id, sent) = self.pending.take()?;
        if id != request_id {
            self.pending = Some((id, sent));
            return None;
        }
        self.missed = 0;
        Some(u64::try_from(now.saturating_duration_since(sent).as_millis()).unwrap_or(u64::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_unanswered_ticks_are_dead_and_a_pong_resets() {
        let t0 = Instant::now();
        let mut p = Pinger::default();
        assert!(matches!(p.on_tick(t0, 0), Tick::Send(_)));
        assert!(matches!(p.on_tick(t0, 0), Tick::Send(_))); // 1 missed
        assert!(matches!(p.on_tick(t0, 0), Tick::Dead)); // 2 missed

        let mut p = Pinger::default();
        assert!(matches!(p.on_tick(t0, 0), Tick::Send(_)));
        assert!(matches!(p.on_tick(t0, 0), Tick::Send(_)));
        assert!(p.on_pong("rtt-2", t0).is_some());
        assert!(matches!(p.on_tick(t0, 0), Tick::Send(_)));
        assert!(matches!(p.on_tick(t0, 0), Tick::Send(_)));
    }

    #[test]
    fn foreign_or_duplicate_pongs_are_ignored() {
        let t0 = Instant::now();
        let mut p = Pinger::default();
        assert!(p.on_pong("rtt-1", t0).is_none());
        let _ = p.on_tick(t0, 0);
        assert!(p.on_pong("other", t0).is_none());
        assert!(p.on_pong("rtt-1", t0).is_some());
        assert!(p.on_pong("rtt-1", t0).is_none());
    }
}
