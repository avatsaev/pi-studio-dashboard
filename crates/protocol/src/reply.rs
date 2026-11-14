//! Replies to the two mutations. Both are flat session messages keyed by `requestId`.

use serde_json::Value;

use crate::lenient::cap_str;

/// Wire type of the `RespondUi` reply.
pub const RESPOND_REPLY_TYPE: &str = "agent_ui_respond_response";
/// Wire type of the `InterruptAgent` reply.
pub const INTERRUPT_REPLY_TYPE: &str = "interrupt_response";

const MAX_ERROR_BYTES: usize = 256;

/// `agent_ui_respond_response {payload:{ok, error?}, requestId}`. `ok:false` is a domain
/// result (`error:"not_found"` = unknown or already resolved); other `error` strings stay open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespondUiReply {
    pub request_id: String,
    pub ok: bool,
    pub error: Option<String>,
}

impl RespondUiReply {
    /// `None` (malformed) without a string `requestId` or a boolean `payload.ok`.
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        let request_id = m.get("requestId")?.as_str()?.to_owned();
        let payload = m.get("payload")?;
        let ok = payload.get("ok")?.as_bool()?;
        let error = payload.get("error").and_then(Value::as_str).map(|s| cap_str(s, MAX_ERROR_BYTES).0);
        Some(Self { request_id, ok, error })
    }
}

/// `interrupt_response {agentId, ok, requestId}` (flat).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptReply {
    pub request_id: String,
    pub agent_id: Option<String>,
    pub ok: bool,
}

impl InterruptReply {
    /// `None` (malformed) without a string `requestId` or a boolean `ok`.
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        Some(Self {
            request_id: m.get("requestId")?.as_str()?.to_owned(),
            agent_id: m.get("agentId").and_then(Value::as_str).map(str::to_owned),
            ok: m.get("ok")?.as_bool()?,
        })
    }
}
