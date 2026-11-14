//! Test helpers shared by the workspace. Other crates use this as a dev-dependency only
//! (`cargo xtask layers` checks that), and it depends on no workspace crate as a normal
//! dependency (`protocol` is a dev-dependency for the fixture harness), so there is no
//! dependency cycle.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod fixtures;

use serde_json::{Value, json};

/// Params no command asked for: wrong types, extremes, empties and oversized values. Every
/// command must answer each of them with `Ok` or `Err`, never a panic or a hang.
pub fn adversarial_params() -> Vec<Value> {
    let long = "x".repeat(1 << 20);
    // 10 000 events exercise the batch cap; only the first few carry a 1 MiB string, because
    // every command re-clones this value and one per event needed ~10 GB (OOM-killed panic_hunt).
    let hostile_events: Vec<Value> = (0..10_000)
        .map(|i| {
            let big = if i < 8 { long.as_str() } else { "x" };
            match i % 4 {
                0 => json!({"entryId": "e1", "connEpoch": u64::MAX, "seq": -1, "at": i64::MIN, "kind": {"type": "health", "reachable": true, "at": i64::MAX}}),
                1 => json!({"entryId": big, "connEpoch": 1, "seq": 0, "at": 0, "kind": {"type": "nope"}}),
                2 => json!({"entryId": "e1", "connEpoch": 1, "seq": 0, "at": 0, "kind": {"type": "snapshot", "agents": [{"agentId": big, "status": big}], "uiPending": [{"requestId": "r", "expectsResponse": true, "title": big}]}}),
                _ => json!({"entryId": "e1", "serverId": "s", "connEpoch": 2, "seq": 0, "at": 0, "kind": {"type": "hello", "info": {}, "caps": {}, "degraded": [], "tlsInUse": false, "isLoopback": false}}),
            }
        })
        .collect();
    vec![
        json!({}),
        json!({"name": 5, "host": [], "port": "x", "tls": "yes", "webUiUrl": {}, "password": 1, "remember": "no", "error": [], "events": -1, "entryId": 7, "serverId": [], "agentId": {}, "requestId": true, "filter": {}, "filterDaemons": "x", "filterStatuses": 3, "gridWindow": "w", "railCollapsed": "yes"}),
        json!({"name": "n", "host": "h", "port": 70000, "webUiUrl": "ftp://x", "password": "p w", "remember": true, "events": [], "entryId": "", "serverId": "", "agentId": "", "requestId": "", "filterDaemons": [], "gridWindow": {"start": u64::MAX, "count": u64::MAX}}),
        json!({"name": long, "host": long, "port": u64::MAX, "password": long, "remember": false, "error": long, "events": 1e300, "entryId": long, "serverId": long, "filter": long, "filterDaemons": [long], "filterStatuses": [long], "gridWindow": {"start": 1e300, "count": -1}}),
        json!({"name": "\u{202e}", "host": "a:b", "port": 0, "password": "", "webUiUrl": null, "autoDetected": true, "events": "\u{202e}\u{0}\n", "entryId": "..", "serverId": "\u{0}", "agentId": "\u{202e}", "filter": "\\"}),
        json!({"events": {"a": 1}, "filterDaemons": {"a": 1}, "filterStatuses": [1], "gridWindow": [1, 2], "railCollapsed": [true], "unknown": true}),
        json!({"events": hostile_events}),
    ]
}
