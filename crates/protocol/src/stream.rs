//! `agent_stream` pushes and the stream events inside timeline pages.
//!
//! Decoding is lenient: unknown fields are ignored, a wrong-typed optional field is `None`, an
//! unknown `event.kind` becomes [`StreamEvent::Unknown`] (counted by the caller, never fatal).
//! The serde derives describe the *internal* event JSON (tag `type`), not the wire format;
//! wire decoding is [`StreamEvent::from_value`].

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lenient::{cap_str, wire_time_ms};

/// Message, reasoning and tool output text (D28).
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
/// Error and detail strings.
pub const MAX_ERROR_BYTES: usize = 1024;
const MAX_ID_BYTES: usize = 256;

/// What a `tool_call` event says about its tool: its kind, one detail line and any output.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolInfo {
    pub kind: String,
    /// The first of `command`, `path`, `query`, `url`, `description` the daemon sent.
    pub detail: Option<String>,
    pub output: Option<String>,
}

/// One event of an agent's stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum StreamEvent {
    UserMessage {
        #[serde(default)]
        text: Option<String>,
    },
    AssistantMessage {
        #[serde(default)]
        text: String,
        #[serde(default, rename = "final")]
        is_final: bool,
    },
    Reasoning {
        #[serde(default)]
        text: Option<String>,
        /// `final: true` closes the block (no text).
        #[serde(default, rename = "final")]
        is_final: bool,
    },
    ToolCall {
        call_id: String,
        /// Free string: `started`, `running`, `completed`, `error`, `canceled`, …
        status: String,
        #[serde(default)]
        partial: bool,
        #[serde(default)]
        tool: ToolInfo,
    },
    TurnStarted,
    TurnCompleted,
    TurnFailed {
        #[serde(default)]
        error: Option<String>,
    },
    TurnCanceled,
    Error {
        #[serde(default)]
        message: Option<String>,
    },
    Compaction {
        #[serde(default)]
        phase: Option<String>,
        #[serde(default)]
        reason: Option<String>,
    },
    /// A kind this build does not know.
    Unknown {
        kind: String,
    },
}

fn text(o: &serde_json::Map<String, Value>, k: &str, max: usize) -> Option<String> {
    o.get(k).and_then(Value::as_str).map(|s| cap_str(s, max).0)
}

impl StreamEvent {
    /// Frequent, order-insensitive-enough events that may be shed under pressure: text deltas
    /// and partial tool output. Everything else is attention-class.
    #[must_use]
    pub fn is_foldable(&self) -> bool {
        matches!(self, Self::AssistantMessage { is_final: false, .. } | Self::Reasoning { is_final: false, .. } | Self::ToolCall { partial: true, .. })
    }

    /// Decode an `event` object. `None` when it has no string `kind` or a known kind lacks a
    /// required field (`tool_call` without `callId`).
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        let kind = o.get("kind")?.as_str()?;
        Some(match kind {
            "user_message" => Self::UserMessage { text: text(o, "text", MAX_TEXT_BYTES) },
            "assistant_message" => Self::AssistantMessage {
                text: text(o, "text", MAX_TEXT_BYTES).unwrap_or_default(),
                is_final: o.get("final").and_then(Value::as_bool).unwrap_or(false),
            },
            "reasoning" => Self::Reasoning {
                text: text(o, "text", MAX_TEXT_BYTES).or_else(|| text(o, "delta", MAX_TEXT_BYTES)),
                is_final: o.get("final").and_then(Value::as_bool).unwrap_or(false),
            },
            "tool_call" => {
                let tool = o.get("tool").and_then(Value::as_object);
                let detail = tool.and_then(|t| ["command", "path", "query", "url", "description"].iter().find_map(|k| text(t, k, MAX_ERROR_BYTES)));
                Self::ToolCall {
                    call_id: text(o, "callId", MAX_ID_BYTES).filter(|s| !s.is_empty())?,
                    status: text(o, "status", MAX_ID_BYTES).unwrap_or_default(),
                    partial: o.get("partial").and_then(Value::as_bool).unwrap_or(false),
                    tool: ToolInfo {
                        kind: tool.and_then(|t| text(t, "kind", MAX_ID_BYTES)).unwrap_or_default(),
                        detail,
                        output: tool.and_then(|t| text(t, "output", MAX_TEXT_BYTES)),
                    },
                }
            }
            "turn_started" => Self::TurnStarted,
            "turn_completed" => Self::TurnCompleted,
            "turn_failed" => Self::TurnFailed { error: text(o, "error", MAX_ERROR_BYTES) },
            "turn_canceled" => Self::TurnCanceled,
            "error" => Self::Error { message: text(o, "message", MAX_ERROR_BYTES) },
            "compaction" => Self::Compaction { phase: text(o, "phase", MAX_ID_BYTES), reason: text(o, "reason", MAX_ID_BYTES) },
            other => Self::Unknown { kind: cap_str(other, MAX_ID_BYTES).0 },
        })
    }
}

/// `agent_stream {agentId, seq?, timestamp?, event}`. Partial tool calls and deltas carry no `seq`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStream {
    pub agent_id: String,
    pub seq: Option<u64>,
    /// Epoch ms (the wire sends ISO-8601).
    pub timestamp: Option<i64>,
    pub event: StreamEvent,
}

impl AgentStream {
    /// `None` (malformed) without `agentId` or a decodable `event`.
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        Some(Self {
            agent_id: text(m.as_object()?, "agentId", MAX_ID_BYTES).filter(|s| !s.is_empty())?,
            seq: m.get("seq").and_then(Value::as_u64),
            timestamp: m.get("timestamp").and_then(wire_time_ms),
            event: StreamEvent::from_value(m.get("event")?)?,
        })
    }
}
