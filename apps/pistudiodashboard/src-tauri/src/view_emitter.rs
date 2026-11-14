//! Pure timing of `view` events (D28): at most one per window, and the last state is never lost.
//!
//! The owner thread asks [`ViewEmitter::on_request`] after each request: `true` means build and
//! emit the view now; otherwise a trailing emit is due at [`ViewEmitter::deadline`], at the
//! latest one window after the previous emit. Times are caller-supplied milliseconds (an
//! injected clock keeps this testable).

/// The emit window (D28: at most 10 views per second).
pub const WINDOW_MS: u64 = 100;

#[derive(Debug, Clone)]
pub struct ViewEmitter {
    window_ms: u64,
    last_emit: Option<u64>,
    pending: bool,
}

impl ViewEmitter {
    pub fn new(window_ms: u64) -> Self {
        Self { window_ms, last_emit: None, pending: false }
    }

    fn due(&self, now: u64) -> bool {
        self.last_emit.is_none_or(|last| now >= last.saturating_add(self.window_ms))
    }

    /// A request was answered. `true`: emit now. `false`: a trailing emit is scheduled.
    pub fn on_request(&mut self, now: u64) -> bool {
        if self.due(now) {
            true
        } else {
            self.pending = true;
            false
        }
    }

    /// When a scheduled trailing emit is due, if one is.
    pub fn deadline(&self) -> Option<u64> {
        self.pending.then(|| self.last_emit.map_or(0, |last| last.saturating_add(self.window_ms)))
    }

    /// The wait expired: `true` if the trailing emit should happen now.
    pub fn on_tick(&mut self, now: u64) -> bool {
        self.pending && self.due(now)
    }

    /// A view was emitted at `now`.
    pub fn emitted(&mut self, now: u64) {
        self.last_emit = Some(now);
        self.pending = false;
    }

    /// The view turned out to be unchanged: nothing was emitted and nothing is owed.
    pub fn unchanged(&mut self) {
        self.pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_request_emits_and_a_burst_waits_for_the_window() {
        let mut e = ViewEmitter::new(100);
        assert!(e.on_request(0));
        e.emitted(0);
        assert!(!e.on_request(10));
        assert!(!e.on_request(50));
        assert_eq!(e.deadline(), Some(100));
        assert!(!e.on_tick(99));
        assert!(e.on_tick(100));
        e.emitted(100);
        assert_eq!(e.deadline(), None);
        assert!(!e.on_tick(500), "nothing owed");
    }

    #[test]
    fn a_click_in_the_middle_of_a_window_waits_at_most_one_window() {
        let mut e = ViewEmitter::new(100);
        e.emitted(1000);
        assert!(!e.on_request(1040));
        let deadline = e.deadline().unwrap_or(0);
        assert!(deadline - 1040 <= 100);
        assert!(e.on_request(1100), "a request after the window emits at once");
    }

    #[test]
    fn a_thousand_requests_in_a_second_emit_at_most_ten_views() {
        let mut e = ViewEmitter::new(100);
        let mut emits = 0;
        for i in 0..1000u64 {
            let now = i; // 1 request per millisecond
            if let Some(d) = e.deadline()
                && now >= d
                && e.on_tick(now)
            {
                e.emitted(now);
                emits += 1;
            }
            if e.on_request(now) {
                e.emitted(now);
                emits += 1;
            }
        }
        // the trailing emit after the burst
        if let Some(d) = e.deadline()
            && e.on_tick(d)
        {
            e.emitted(d);
            emits += 1;
        }
        assert!(emits <= 11, "{emits}");
        assert!(e.deadline().is_none(), "the last state was emitted");
    }

    #[test]
    fn an_unchanged_view_clears_the_debt() {
        let mut e = ViewEmitter::new(100);
        e.emitted(0);
        assert!(!e.on_request(5));
        e.unchanged();
        assert_eq!(e.deadline(), None);
    }
}
