//! Reconnect schedule (D25): `min(30 s, 0.5 s * 2^attempt) * (1 +/- 0.2)`.

use std::time::Duration;

const BASE_MS: u64 = 500;
const CAP_MS: u64 = 30_000;
/// Jitter spread: the factor is uniform in `[1 - JITTER, 1 + JITTER)`.
const JITTER: f64 = 0.2;

/// The first attempt number whose delay is the 30 s cap.
pub const CAP_ATTEMPT: u32 = 6;

/// How long a connection must have been live before the attempt counter resets.
pub const RESET_AFTER_LIVE: Duration = Duration::from_secs(10);

/// Pure schedule. `unit` is a uniform sample in `[0, 1)` (clamped), injected so tests need no RNG.
#[must_use]
pub fn delay(attempt: u32, unit: f64) -> Duration {
    let base = BASE_MS.saturating_mul(1_u64 << attempt.min(20)).min(CAP_MS);
    let unit = if unit.is_finite() { unit.clamp(0.0, 1.0) } else { 0.5 };
    let factor = 1.0 - JITTER + 2.0 * JITTER * unit;
    // base <= 30_000 and factor < 1.2, so the product is tiny and non-negative.
    Duration::from_millis((base as f64 * factor).round() as u64)
}

/// A uniform sample in `[0, 1)` from the OS; `0.5` (no jitter) if the OS refuses.
pub(crate) fn random_unit() -> f64 {
    let mut b = [0_u8; 4];
    if getrandom::fill(&mut b).is_err() {
        return 0.5;
    }
    f64::from(u32::from_le_bytes(b)) / (f64::from(u32::MAX) + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_without_jitter_doubles_to_the_cap() {
        let ms: Vec<u128> = (0..9).map(|a| delay(a, 0.5).as_millis()).collect();
        assert_eq!(ms, [500, 1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000]);
    }

    #[test]
    fn jitter_stays_within_twenty_percent() {
        for attempt in 0..12 {
            let mid = delay(attempt, 0.5).as_millis() as f64;
            for unit in [0.0, 0.25, 0.999_999, 1.0, -3.0, f64::NAN] {
                let d = delay(attempt, unit).as_millis() as f64;
                assert!(d >= mid * 0.8 - 1.0 && d <= mid * 1.2 + 1.0, "attempt {attempt} unit {unit}: {d} vs {mid}");
            }
        }
    }

    #[test]
    fn cap_attempt_is_the_cap() {
        assert_eq!(delay(CAP_ATTEMPT, 0.5), Duration::from_secs(30));
        assert!(delay(CAP_ATTEMPT - 1, 0.5) < Duration::from_secs(30));
    }

    #[test]
    fn huge_attempt_does_not_overflow() {
        assert_eq!(delay(u32::MAX, 0.5), Duration::from_secs(30));
    }

    #[test]
    fn random_unit_is_in_range() {
        for _ in 0..100 {
            let u = random_unit();
            assert!((0.0..1.0).contains(&u));
        }
    }
}
