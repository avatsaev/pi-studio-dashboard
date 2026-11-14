//! `rpc_error`: the daemon's error reply, carried in a `session` message.

use serde_json::Value;

use crate::lenient::cap_str;

const MAX_CODE_BYTES: usize = 256;
const MAX_TEXT_BYTES: usize = 2048;

/// Decoded session message of type `rpc_error`. `code` is an open string
/// (`unknown_message_type`, `handler_error`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcError {
    pub request_id: String,
    pub code: Option<String>,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl RpcError {
    /// `None` when `requestId` is missing (the message is then malformed). Wrong-typed
    /// optional fields become `None`; unknown fields are ignored.
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        let o = m.as_object()?;
        let text = |k: &str, max: usize| o.get(k).and_then(Value::as_str).map(|s| cap_str(s, max).0);
        Some(Self {
            request_id: o.get("requestId").and_then(Value::as_str)?.to_owned(),
            code: text("code", MAX_CODE_BYTES),
            message: text("message", MAX_TEXT_BYTES),
            error: text("error", MAX_TEXT_BYTES),
        })
    }
}
