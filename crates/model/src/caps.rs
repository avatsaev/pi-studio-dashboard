//! Every bound from the live-state spec. Byte counts (not chars): a conservative bound that is
//! cheap to check; truncation happens on a UTF-8 boundary.

use pistudiodashboard_protocol::cap_str;

/// Events kept per daemon.
pub const EVENT_RING: usize = 500;
/// Drill-down items kept per buffer.
pub const DRILL_ITEMS: usize = 2000;
/// Stream events held while a drill buffer is loading.
pub const DRILL_HOLD: usize = 2000;
/// Dialog title.
pub const DIALOG_TITLE: usize = 200;
/// Dialog message (confirm).
pub const DIALOG_MESSAGE: usize = 500;
/// One dialog option.
pub const DIALOG_OPTION: usize = 120;
/// Dialog options count.
pub const DIALOG_OPTIONS: usize = 50;
/// Agent title.
pub const SUMMARY_TITLE: usize = 200;
/// Labels and tool labels.
pub const LABEL: usize = 120;
/// Last output line.
pub const OUTPUT_LINE: usize = 200;
/// Error text.
pub const ERROR_TEXT: usize = 200;
/// Assistant text tail per agent (reducer buffer).
pub const ASSISTANT_TAIL: usize = 1024;
/// Tool output tail lines kept.
pub const TOOL_OUTPUT_TAIL_LINES: usize = 12;
/// One tool output tail line.
pub const TOOL_OUTPUT_LINE: usize = 500;

/// Truncate to `max` bytes on a UTF-8 boundary (wraps the L0 helper, dropping the flag).
#[must_use]
pub fn cap_string(s: &str, max: usize) -> String {
    cap_str(s, max).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_string_respects_char_boundary() {
        assert_eq!(cap_string("héllo", 2), "h");
        assert_eq!(cap_string("abc", 10), "abc");
    }
}
