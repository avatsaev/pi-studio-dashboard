//! Plain data types of the config file.

use serde::Serialize;
use serde_json::{Map, Value};

/// Highest `version` this build understands.
pub const SUPPORTED_VERSION: u32 = 1;
/// At most this many daemon entries are loaded.
pub const MAX_DAEMONS: usize = 50;
/// Larger files are treated as corrupt.
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
/// Port used when an entry has no `port` key.
pub const DEFAULT_PORT: u16 = 6767;

/// Unknown keys of one JSON object, preserved verbatim (sorted by key on output).
pub type Extra = Map<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub version: u32,
    pub daemons: Vec<DaemonEntry>,
    pub settings: Settings,
    pub local_detection_dismissed: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Config {
    fn default() -> Self {
        Self { version: SUPPORTED_VERSION, daemons: Vec::new(), settings: Settings::default(), local_detection_dismissed: false, extra: Extra::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonEntry {
    pub entry_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_ui_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_server_id: Option<String>,
    pub auto_detected: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notifications {
    pub needs_input: bool,
    pub failures: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Notifications {
    fn default() -> Self {
        Self { needs_input: true, failures: true, extra: Extra::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: ThemeChoice,
    pub notifications: Notifications,
    pub close_to_tray: bool,
    pub attention_rail_collapsed: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Settings {
    fn default() -> Self {
        Self { theme: ThemeChoice::System, notifications: Notifications::default(), close_to_tray: true, attention_rail_collapsed: false, extra: Extra::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    DroppedEntry,
    DefaultedValue,
    Capped,
    CorruptFile,
    FutureVersion,
    Io,
}

/// One thing that was repaired, dropped or could not be done while loading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigIssue {
    pub kind: IssueKind,
    /// JSON path of the offending value, e.g. `daemons[2].port`; empty for the whole file.
    pub path: String,
    pub detail: String,
}

impl ConfigIssue {
    pub(crate) fn new(kind: IssueKind, path: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { kind, path: path.into(), detail: detail.into() }
    }
}

/// Builds a UUID-v4-format id from 16 caller-supplied random bytes.
pub fn new_entry_id(mut random: [u8; 16]) -> String {
    random[6] = (random[6] & 0x0f) | 0x40;
    random[8] = (random[8] & 0x3f) | 0x80;
    let mut out = String::with_capacity(36);
    for (i, b) in random.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push(char::from_digit(u32::from(b >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(b & 0x0f), 16).unwrap_or('0'));
    }
    out
}

/// True when `s` is a UUID of version 4 (any hex case).
pub(crate) fn is_uuid_v4(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    b.iter().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => *c == b'-',
        14 => *c == b'4',
        19 => matches!(*c, b'8' | b'9' | b'a' | b'b' | b'A' | b'B'),
        _ => c.is_ascii_hexdigit(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_id_is_v4() {
        assert!(is_uuid_v4(&new_entry_id([0xff; 16])));
        assert!(is_uuid_v4(&new_entry_id([0; 16])));
        assert!(!is_uuid_v4("nope"));
    }
}
