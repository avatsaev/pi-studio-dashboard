//! Per-entry diagnostics counters (D29). They live as long as the entry's task: they reset on
//! process start, not on reconnect.

use std::time::Duration;

use pistudiodashboard_model::CountersSnapshot;
use pistudiodashboard_protocol::cap_str;
use tokio::time::Instant;

/// Distinct `unknownTypes` keys kept before further types land in [`OTHER`].
pub(crate) const MAX_UNKNOWN_KEYS: usize = 64;
pub(crate) const OTHER: &str = "other";
const MAX_KEY_BYTES: usize = 64;
const RATE_WINDOW: Duration = Duration::from_secs(1);
/// EWMA weight of the newest RTT sample.
const EWMA_ALPHA: f64 = 0.2;

pub(crate) struct Counters {
    snap: CountersSnapshot,
    dirty: bool,
    window_start: Instant,
    window_count: u32,
}

impl Counters {
    pub(crate) fn new(now: Instant) -> Self {
        Self { snap: CountersSnapshot::default(), dirty: false, window_start: now, window_count: 0 }
    }

    fn touch(&mut self) {
        self.dirty = true;
    }

    /// Close the 1 s window if it elapsed.
    fn roll(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.window_start);
        if elapsed >= RATE_WINDOW {
            // More than one window of silence means the rate is zero.
            self.snap.messages_per_second = if elapsed >= RATE_WINDOW * 2 { 0 } else { self.window_count };
            self.window_start = now;
            self.window_count = 0;
        }
    }

    /// A data frame (text or binary) arrived.
    pub(crate) fn message(&mut self, now: Instant) {
        self.roll(now);
        self.window_count = self.window_count.saturating_add(1);
        self.snap.messages_total += 1;
        self.touch();
    }

    pub(crate) fn malformed(&mut self) {
        self.snap.malformed += 1;
        self.touch();
    }

    pub(crate) fn binary_ignored(&mut self) {
        self.snap.binary_frames_ignored += 1;
        self.touch();
    }

    pub(crate) fn frame_too_large(&mut self) {
        self.snap.frames_too_large += 1;
        self.touch();
    }

    /// Count an unknown `type`; bounded to [`MAX_UNKNOWN_KEYS`] keys, then [`OTHER`].
    pub(crate) fn unknown_type(&mut self, ty: &str) {
        let key = cap_str(ty, MAX_KEY_BYTES).0;
        let key = if self.snap.unknown_types.contains_key(&key) || self.snap.unknown_types.len() < MAX_UNKNOWN_KEYS { key } else { OTHER.to_owned() };
        *self.snap.unknown_types.entry(key).or_insert(0) += 1;
        self.touch();
    }

    /// Called by typed decoders when a wire enum fell back to `Unknown`.
    #[allow(dead_code)]
    pub(crate) fn unknown_enum_value(&mut self) {
        self.snap.unknown_enum_values += 1;
        self.touch();
    }

    #[allow(dead_code)]
    pub(crate) fn dropped_event(&mut self) {
        self.snap.dropped_events += 1;
        self.touch();
    }

    #[allow(dead_code)]
    pub(crate) fn late_response(&mut self) {
        self.snap.late_responses += 1;
        self.touch();
    }

    pub(crate) fn reconnect(&mut self) {
        self.snap.reconnect_count += 1;
        self.touch();
    }

    /// Record an RTT sample; returns the new EWMA.
    pub(crate) fn rtt(&mut self, ms: u64) {
        let ewma = match self.snap.rtt_ewma_ms {
            None => ms,
            Some(prev) => (EWMA_ALPHA * ms as f64 + (1.0 - EWMA_ALPHA) * prev as f64).round() as u64,
        };
        self.snap.rtt_last_ms = Some(ms);
        self.snap.rtt_ewma_ms = Some(ewma);
        self.touch();
    }

    pub(crate) fn connected(&mut self, at_ms: i64) {
        self.snap.connected_since = Some(at_ms);
        self.touch();
    }

    pub(crate) fn disconnected(&mut self) {
        if self.snap.connected_since.take().is_some() {
            self.touch();
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The snapshot to emit; clears the dirty flag.
    pub(crate) fn take_snapshot(&mut self, now: Instant) -> CountersSnapshot {
        self.roll(now);
        self.dirty = false;
        self.snap.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_types_are_bounded_then_bucketed() {
        let mut c = Counters::new(Instant::now());
        for i in 0..(MAX_UNKNOWN_KEYS + 10) {
            c.unknown_type(&format!("type_{i}"));
        }
        c.unknown_type("type_0");
        let s = c.take_snapshot(Instant::now());
        assert_eq!(s.unknown_types.len(), MAX_UNKNOWN_KEYS + 1);
        assert_eq!(s.unknown_types[OTHER], 10);
        assert_eq!(s.unknown_types["type_0"], 2);
    }

    #[test]
    fn long_type_names_are_capped() {
        let mut c = Counters::new(Instant::now());
        c.unknown_type(&"é".repeat(500));
        assert!(c.take_snapshot(Instant::now()).unknown_types.keys().all(|k| k.len() <= MAX_KEY_BYTES));
    }

    #[test]
    fn ewma_follows_the_documented_weight() {
        let mut c = Counters::new(Instant::now());
        c.rtt(100);
        c.rtt(200);
        let s = c.take_snapshot(Instant::now());
        assert_eq!((s.rtt_last_ms, s.rtt_ewma_ms), (Some(200), Some(120)));
    }

    #[tokio::test(start_paused = true)]
    async fn rate_uses_a_one_second_window_and_dirty_clears_on_snapshot() {
        let mut c = Counters::new(Instant::now());
        assert!(!c.is_dirty());
        for _ in 0..5 {
            c.message(Instant::now());
        }
        assert!(c.is_dirty());
        tokio::time::advance(Duration::from_millis(1100)).await;
        let s = c.take_snapshot(Instant::now());
        assert_eq!((s.messages_total, s.messages_per_second), (5, 5));
        assert!(!c.is_dirty());
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(c.take_snapshot(Instant::now()).messages_per_second, 0);
    }
}
