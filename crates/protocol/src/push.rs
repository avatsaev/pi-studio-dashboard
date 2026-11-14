//! Typed server pushes the client forwards itself.

use serde_json::Value;

use crate::lenient::cap_str;

const MAX_FIELD_BYTES: usize = 512;

/// `agent_ui_resolved {requestId, agentId, reason}`; `reason` is an open string
/// (`answered`, `timeout`, `aborted`, `cancelled`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiResolved {
    pub request_id: String,
    pub agent_id: String,
    pub reason: String,
}

impl UiResolved {
    /// `None` (malformed) without the three string fields.
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        let text = |k: &str| m.get(k)?.as_str().map(|s| cap_str(s, MAX_FIELD_BYTES).0);
        Some(Self { request_id: text("requestId")?, agent_id: text("agentId")?, reason: text("reason")? })
    }
}
