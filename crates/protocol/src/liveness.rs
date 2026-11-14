//! JSON liveness: top-level `ping` (encoder) and `pong` (decoder). Never RFC 6455 ping.

use serde_json::{Map, Value};

use crate::lenient::wire_time_ms;

/// Outbound `ping`; the daemon answers with a `pong` carrying the same `requestId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ping {
    pub request_id: String,
    /// Epoch milliseconds.
    pub client_sent_at: i64,
}

impl Ping {
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut o = Map::new();
        o.insert("type".into(), "ping".into());
        o.insert("requestId".into(), self.request_id.clone().into());
        o.insert("clientSentAt".into(), self.client_sent_at.into());
        Value::Object(o)
    }

    #[must_use]
    pub fn encode(&self) -> String {
        self.to_value().to_string()
    }
}

/// Decoded `pong`; all timestamps are epoch ms (numbers or ISO strings on the wire).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pong {
    pub request_id: String,
    pub server_received_at: i64,
    pub server_sent_at: i64,
    pub client_sent_at: Option<i64>,
}

impl Pong {
    /// Build from the optional fields of a decoded top-level `pong`; `None` when a required
    /// field (`requestId`, `serverReceivedAt`, `serverSentAt`) is missing or unparsable.
    #[must_use]
    pub fn from_parts(request_id: Option<String>, server_received_at: Option<i64>, server_sent_at: Option<i64>, client_sent_at: Option<i64>) -> Option<Self> {
        Some(Self { request_id: request_id?, server_received_at: server_received_at?, server_sent_at: server_sent_at?, client_sent_at })
    }

    /// Decode a `pong` JSON object directly.
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        Self::from_parts(
            o.get("requestId").and_then(Value::as_str).map(str::to_owned),
            o.get("serverReceivedAt").and_then(wire_time_ms),
            o.get("serverSentAt").and_then(wire_time_ms),
            o.get("clientSentAt").and_then(wire_time_ms),
        )
    }
}
