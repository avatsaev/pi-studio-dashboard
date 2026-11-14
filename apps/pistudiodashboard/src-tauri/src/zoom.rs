//! Zoom of the window's content. The shell owns the level (`ui.zoom`, a percent, saved in the
//! config file); this applies it to the webview, which scales like a browser's page zoom (so the
//! layout, the card grid's window maths included, sees a smaller or larger viewport in CSS pixels).
//! Doing it here, in the app binary, needs no webview permission.

use std::sync::atomic::{AtomicU16, Ordering};

use serde_json::Value;

/// The level last seen in a view, kept for the moment a page finishes loading (a zoom set before
/// the page exists is not reliably kept by the platform webviews).
pub struct Zoom(AtomicU16);

impl Zoom {
    pub fn new() -> Self {
        Self(AtomicU16::new(100))
    }

    /// Remember `percent`; true when it differs from the one before.
    pub fn set(&self, percent: u16) -> bool {
        self.0.swap(percent, Ordering::Relaxed) != percent
    }

    pub fn get(&self) -> u16 {
        self.0.load(Ordering::Relaxed)
    }
}

/// `ui.zoom` of a view as the shell sends it.
pub fn percent_of(view: &Value) -> Option<u16> {
    view.pointer("/ui/zoom").and_then(Value::as_u64).and_then(|z| u16::try_from(z).ok())
}

/// The webview's scale factor for a percent.
pub fn factor(percent: u16) -> f64 {
    f64::from(percent) / 100.0
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_level_is_read_from_the_view_and_a_change_is_reported_once() {
        assert_eq!(percent_of(&json!({"ui": {"zoom": 125}})), Some(125));
        assert_eq!(percent_of(&json!({"ui": {}})), None);
        assert_eq!(percent_of(&json!({"ui": {"zoom": -1}})), None);
        assert_eq!(percent_of(&json!({"ui": {"zoom": 70000}})), None);
        let z = Zoom::new();
        assert_eq!(z.get(), 100);
        assert!(!z.set(100));
        assert!(z.set(150));
        assert!(!z.set(150));
        assert_eq!(z.get(), 150);
    }

    #[test]
    fn percent_becomes_a_scale_factor() {
        assert!((factor(100) - 1.0).abs() < f64::EPSILON);
        assert!((factor(125) - 1.25).abs() < 1e-9);
        assert!((factor(50) - 0.5).abs() < f64::EPSILON);
    }
}
