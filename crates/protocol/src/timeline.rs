//! `fetch_agent_timeline_response`: one page of an agent's timeline.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lenient::{cap_str, wire_time_ms};
use crate::stream::StreamEvent;

/// One timeline event. `kind` is the projection label of the item it came from (`other`,
/// `assistant`, `tool_call`), not the event kind; collapsed items are flattened to one entry per
/// event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub kind: String,
    pub source_seq: u64,
    /// Epoch ms.
    #[serde(default)]
    pub timestamp: Option<i64>,
    pub event: StreamEvent,
}

/// A decoded page. Cursors are opaque strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelinePage {
    pub request_id: String,
    pub agent_id: Option<String>,
    pub items: Vec<TimelineItem>,
    pub seq_start: Option<u64>,
    pub seq_end: u64,
    pub has_newer: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
    /// Items dropped because they could not be decoded at all.
    pub malformed: u32,
    /// Items whose event kind is not known; kept as `StreamEvent::Unknown`.
    pub unknown_kinds: u32,
}

impl TimelinePage {
    /// `None` (malformed) without `requestId`, an `items` array, `seqEnd` or `hasNewer`.
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        let request_id = m.get("requestId")?.as_str()?.to_owned();
        let list = m.get("items")?.as_array()?;
        let seq_end = m.get("seqEnd")?.as_u64()?;
        let has_newer = m.get("hasNewer")?.as_bool()?;
        let (mut items, mut malformed, mut unknown_kinds) = (Vec::with_capacity(list.len()), 0, 0);
        for it in list {
            let kind = it.get("kind").and_then(Value::as_str).map(|s| cap_str(s, 64).0).unwrap_or_default();
            let timestamp = it.get("timestamp").and_then(wire_time_ms);
            // `other` items carry one `event`; `assistant` / `tool_call` items collapse a run of
            // adjacent rows into `events[]` with a `sourceSeqStart..End` range.
            let source_seq = it.get("sourceSeq").or_else(|| it.get("sourceSeqEnd")).and_then(Value::as_u64);
            let raw: Vec<&Value> = match (it.get("event"), it.get("events").and_then(Value::as_array)) {
                (Some(e), _) => vec![e],
                (None, Some(events)) => events.iter().collect(),
                (None, None) => Vec::new(),
            };
            let Some(source_seq) = source_seq.filter(|_| !raw.is_empty()) else {
                malformed += 1;
                continue;
            };
            for e in raw {
                match StreamEvent::from_value(e) {
                    Some(event) => {
                        unknown_kinds += u32::from(matches!(event, StreamEvent::Unknown { .. }));
                        items.push(TimelineItem { kind: kind.clone(), source_seq, timestamp, event });
                    }
                    None => malformed += 1,
                }
            }
        }
        let cursor = |k: &str| m.get(k).and_then(Value::as_str).map(|s| cap_str(s, 128).0);
        Some(Self {
            request_id,
            agent_id: m.get("agentId").and_then(Value::as_str).map(|s| cap_str(s, 256).0),
            items,
            seq_start: m.get("seqStart").and_then(Value::as_u64),
            seq_end,
            has_newer,
            start_cursor: cursor("startCursor"),
            end_cursor: cursor("endCursor"),
            malformed,
            unknown_kinds,
        })
    }
}
