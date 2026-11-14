//! Zoom of the window's content, in percent. The app binary applies `UiState.zoom` to the webview;
//! this module only knows the steps `Zoom In` and `Zoom Out` move through (the usual browser ones),
//! so any value in range can be set (`ui.set`) and the next step is still well defined.

use pistudiodashboard_engine::{ZOOM_DEFAULT, ZOOM_MAX, ZOOM_MIN};

pub const STEPS: [u16; 13] = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];

/// The default zoom (`Actual Size`).
pub const DEFAULT: u16 = ZOOM_DEFAULT;

/// The next larger step, or `None` at the largest.
#[must_use]
pub fn zoom_in(current: u16) -> Option<u16> {
    STEPS.iter().copied().find(|s| *s > current).filter(|s| *s <= ZOOM_MAX)
}

/// The next smaller step, or `None` at the smallest.
#[must_use]
pub fn zoom_out(current: u16) -> Option<u16> {
    STEPS.iter().rev().copied().find(|s| *s < current).filter(|s| *s >= ZOOM_MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_sorted_and_inside_the_allowed_range() {
        assert!(STEPS.windows(2).all(|w| w[0] < w[1]));
        assert_eq!((STEPS.first().copied(), STEPS.last().copied()), (Some(ZOOM_MIN), Some(ZOOM_MAX)));
        assert!(STEPS.contains(&DEFAULT));
    }

    #[test]
    fn in_and_out_move_one_step_and_stop_at_the_ends() {
        assert_eq!((zoom_in(100), zoom_out(100)), (Some(110), Some(90)));
        assert_eq!((zoom_in(300), zoom_out(50)), (None, None));
        assert_eq!((zoom_in(299), zoom_out(51)), (Some(300), Some(50)));
    }

    #[test]
    fn a_value_between_steps_moves_to_the_neighbouring_step() {
        assert_eq!((zoom_in(120), zoom_out(120)), (Some(125), Some(110)));
        assert_eq!((zoom_in(66), zoom_out(66)), (Some(67), Some(50)));
    }
}
