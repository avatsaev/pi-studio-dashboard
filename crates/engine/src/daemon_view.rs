//! The daemon-specific parts of the monitor view: chip extras and the per-daemon diagnostics
//! (daemon-connections § "Unencrypted" badge, § Degraded mode, § Diagnostics panel).
//!
//! Built only from `MonitorState`, the config entry and the password flags. Never included:
//! a password, a bearer, the subprotocol, `action`/dialog payload text or a raw frame (D29, D32);
//! nothing in the slice holds those, and the last error is the client's secret-free text.

use std::net::IpAddr;

use pistudiodashboard_config::DaemonEntry;
use pistudiodashboard_model::{DaemonSlice, DegradedReason};
use serde_json::{Value, json};

use crate::Session;

/// Features this dashboard looks for in `server_info.features`; one that is absent is listed as missing.
const KNOWN_FEATURES: [&str; 1] = ["daemonObservability"];

/// `127.0.0.0/8`, `::1` and `localhost` (the host as the config stores it: lowercase, no brackets).
fn is_loopback(host: &str) -> bool {
    host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Plain `ws://` with a password to a non-loopback host: the secret would travel in the clear.
pub(crate) fn unencrypted(entry: Option<&DaemonEntry>, has_password: bool) -> bool {
    entry.is_some_and(|e| !e.tls && has_password && !is_loopback(&e.host))
}

fn degraded(d: &DaemonSlice) -> Vec<&'static str> {
    d.degraded
        .iter()
        .map(|r| match r {
            DegradedReason::NoObservability => "no_observability",
        })
        .collect()
}

/// `daemonChips[].degraded`: reasons the chip shows a glyph for.
pub(crate) fn chip_degraded(d: &DaemonSlice) -> Vec<&'static str> {
    degraded(d)
}

/// `diagnostics` of the monitor view: `null` when `target` names no daemon entry.
pub(crate) fn diagnostics(s: &Session, d: &DaemonSlice) -> Value {
    let entry = s.config().daemons.iter().find(|e| e.entry_id == d.entry_id);
    let c = &d.counters;
    let present: Vec<&str> = d.info.as_ref().map(|i| i.features.iter().map(String::as_str).collect()).unwrap_or_default();
    // A daemon that never said hello has no feature list: nothing is "missing" yet.
    let missing: Vec<&str> = if d.info.is_some() { KNOWN_FEATURES.iter().copied().filter(|f| !present.contains(f)).collect() } else { Vec::new() };
    json!({
        "entryId": d.entry_id,
        "name": d.name,
        "host": entry.map(|e| e.host.clone()),
        "port": entry.map(|e| e.port),
        "tls": entry.is_some_and(|e| e.tls),
        "conn": d.conn.as_str(),
        "since": d.conn_since,
        "lastError": d.last_error.as_ref().map(|e| json!({"kind": e.code, "message": e.message, "at": d.conn_since})),
        "nextRetryInMs": d.next_retry_in_ms,
        "reconnectCount": c.reconnect_count,
        "connectedSince": c.connected_since,
        "messagesPerSecond": c.messages_per_second,
        "messagesTotal": c.messages_total,
        "unknownTypes": c.unknown_types,
        "unknownTypeCount": c.unknown,
        "unknownEnumValues": c.unknown_enum_values,
        "malformed": c.malformed,
        "binaryFramesIgnored": c.binary_frames_ignored,
        "framesTooLarge": c.frames_too_large,
        "droppedEvents": c.dropped_events,
        "rtt": {"lastMs": c.rtt_last_ms, "averageMs": c.rtt_ewma_ms},
        "server": {
            "serverId": d.server_id,
            "hostname": d.info.as_ref().and_then(|i| i.hostname.clone()),
            "version": d.info.as_ref().and_then(|i| i.version.clone()),
            "featuresPresent": present,
            "featuresMissing": missing,
        },
        "duplicateOf": d.duplicate_of,
        "degraded": degraded(d),
        "health": d.health.as_ref().map(|h| json!({"reachable": h.reachable, "at": h.at})),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(host: &str, tls: bool) -> DaemonEntry {
        DaemonEntry {
            entry_id: "a".into(),
            name: "Box".into(),
            host: host.into(),
            port: 6790,
            tls,
            web_ui_url: None,
            last_seen_server_id: None,
            auto_detected: false,
            extra: Default::default(),
        }
    }

    #[test]
    fn unencrypted_needs_plain_ws_a_password_and_a_remote_host() {
        assert!(unencrypted(Some(&entry("10.0.0.5", false)), true));
        assert!(unencrypted(Some(&entry("pi.example.com", false)), true));
        assert!(!unencrypted(Some(&entry("10.0.0.5", true)), true), "wss");
        assert!(!unencrypted(Some(&entry("10.0.0.5", false)), false), "no password");
        for loopback in ["127.0.0.1", "127.4.5.6", "::1", "localhost"] {
            assert!(!unencrypted(Some(&entry(loopback, false)), true), "{loopback}");
        }
        assert!(!unencrypted(None, true));
    }
}
