//! `agent_session_stats_response`: every field beyond `tokens.total` is optional (the mock has
//! no `cost`); `ok:false` means the agent has no live session.

use serde_json::Value;

use crate::lenient::cap_str;

/// Token counts as the daemon reports them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tokens {
    pub total: Option<u64>,
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
}

/// Context-window usage as the daemon reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextUsageWire {
    pub tokens: Option<u64>,
    pub context_window: Option<u64>,
    pub percent: Option<f64>,
}

/// The decoded response. Cost is a bare number: pi-studio sends no unit or currency.
#[derive(Clone, Debug, PartialEq)]
pub struct StatsResponse {
    pub request_id: String,
    /// `false` when `payload.ok == false` (no live session); absent counts as `true`.
    pub ok: bool,
    pub session_id: Option<String>,
    pub total_messages: Option<u64>,
    pub tokens: Option<Tokens>,
    pub cost: Option<f64>,
    pub context_usage: Option<ContextUsageWire>,
    pub model: Option<String>,
    pub error: Option<String>,
}

fn u64_of(o: &Value, k: &str) -> Option<u64> {
    o.get(k).and_then(Value::as_u64)
}

fn f64_of(o: &Value, k: &str) -> Option<f64> {
    o.get(k).and_then(Value::as_f64).filter(|f| f.is_finite())
}

impl StatsResponse {
    /// `None` (malformed) without a string `requestId` or an object `payload`.
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        let request_id = m.get("requestId")?.as_str()?.to_owned();
        let p = m.get("payload").filter(|p| p.is_object())?;
        let text = |k: &str| p.get(k).and_then(Value::as_str).map(|s| cap_str(s, 256).0);
        Some(Self {
            request_id,
            ok: p.get("ok").and_then(Value::as_bool).unwrap_or(true),
            session_id: text("sessionId"),
            total_messages: u64_of(p, "totalMessages"),
            tokens: p.get("tokens").filter(|t| t.is_object()).map(|t| Tokens {
                total: u64_of(t, "total"),
                input: u64_of(t, "input"),
                output: u64_of(t, "output"),
                cache_read: u64_of(t, "cacheRead"),
                cache_write: u64_of(t, "cacheWrite"),
            }),
            cost: f64_of(p, "cost"),
            context_usage: p.get("contextUsage").filter(|c| c.is_object()).map(|c| ContextUsageWire {
                tokens: u64_of(c, "tokens"),
                context_window: u64_of(c, "contextWindow"),
                percent: f64_of(c, "percent"),
            }),
            model: text("model"),
            error: text("error"),
        })
    }
}
