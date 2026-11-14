//! Sync-on-connect wire types: `list_agents_response` and `agent_ui_list_response`.
//!
//! Both are decoded leniently: a wrong-typed optional field becomes `None`, an unknown enum
//! value becomes `Unknown` (and is counted in `unknown_status`), an entry without its required
//! fields is dropped and counted in `malformed`, and the rest of the list survives.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::lenient::{cap_str, wire_time_ms};

/// Agent titles are capped at decode (D28).
const MAX_TITLE_BYTES: usize = 256;
const MAX_FIELD_BYTES: usize = 512;
/// Every string inside a dialog preview is capped at this many bytes (D28, D29).
pub const MAX_PREVIEW_STRING_BYTES: usize = 2048;
/// Option lists (and any other array or object) inside a dialog preview keep this many entries.
pub const MAX_PREVIEW_ENTRIES: usize = 100;
/// Preview nesting beyond this depth is replaced by `null` (bounds work on hostile input).
const MAX_PREVIEW_DEPTH: usize = 8;

/// Agent lifecycle as the daemon reports it; an unknown wire value is [`AgentStatus::Unknown`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentStatus {
    Initializing,
    Idle,
    Running,
    Error,
    Closed,
    Unknown,
}

impl AgentStatus {
    #[must_use]
    pub fn from_wire(s: &str) -> Self {
        match s {
            "initializing" => Self::Initializing,
            "idle" => Self::Idle,
            "running" => Self::Running,
            "error" => Self::Error,
            "closed" => Self::Closed,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Initializing => "initializing",
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Error => "error",
            Self::Closed => "closed",
            Self::Unknown => "unknown",
        }
    }
}

/// One agent of `list_agents_response`.
#[derive(Clone, Debug, PartialEq)]
pub struct ListedAgent {
    pub agent_id: String,
    pub status: AgentStatus,
    pub title: Option<String>,
    pub cwd: Option<String>,
    /// Wire `labels` object; non-scalar values are dropped.
    pub labels: BTreeMap<String, String>,
    /// Epoch ms.
    pub last_activity: Option<i64>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub thinking_level: Option<String>,
}

/// `list_agents_response` (flat: no `payload` wrapper).
#[derive(Clone, Debug, PartialEq)]
pub struct ListAgentsResponse {
    pub request_id: String,
    pub agents: Vec<ListedAgent>,
    /// Agents dropped for a missing `agentId`.
    pub malformed: u32,
    /// Agents whose `status` was not a known value.
    pub unknown_status: u32,
}

fn opt_str(o: &Map<String, Value>, k: &str, max: usize) -> Option<String> {
    o.get(k).and_then(Value::as_str).map(|s| cap_str(s, max).0)
}

fn labels_of(o: &Map<String, Value>) -> BTreeMap<String, String> {
    let Some(Value::Object(m)) = o.get("labels") else { return BTreeMap::new() };
    m.iter()
        .filter_map(|(k, v)| {
            let text = match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => return None,
            };
            Some((cap_str(k, MAX_FIELD_BYTES).0, cap_str(&text, MAX_FIELD_BYTES).0))
        })
        .take(MAX_PREVIEW_ENTRIES)
        .collect()
}

impl ListAgentsResponse {
    /// `None` when `requestId` or the `agents` array is missing (the whole message is malformed).
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        let request_id = m.get("requestId")?.as_str()?.to_owned();
        let list = m.get("agents")?.as_array()?;
        let (mut agents, mut malformed, mut unknown_status) = (Vec::with_capacity(list.len()), 0, 0);
        for item in list {
            let Some(o) = item.as_object() else {
                malformed += 1;
                continue;
            };
            let Some(agent_id) = o.get("agentId").and_then(Value::as_str).filter(|s| !s.is_empty()) else {
                malformed += 1;
                continue;
            };
            let status = match o.get("status").and_then(Value::as_str) {
                Some(s) => {
                    let st = AgentStatus::from_wire(s);
                    if st == AgentStatus::Unknown {
                        unknown_status += 1;
                    }
                    st
                }
                // A missing or wrong-typed status is an unknown value too.
                None => {
                    unknown_status += 1;
                    AgentStatus::Unknown
                }
            };
            agents.push(ListedAgent {
                agent_id: cap_str(agent_id, MAX_FIELD_BYTES).0,
                status,
                title: opt_str(o, "title", MAX_TITLE_BYTES),
                cwd: opt_str(o, "cwd", MAX_FIELD_BYTES),
                labels: labels_of(o),
                last_activity: o.get("lastActivity").and_then(wire_time_ms),
                provider: opt_str(o, "provider", MAX_FIELD_BYTES),
                model: opt_str(o, "model", MAX_FIELD_BYTES),
                model_provider: opt_str(o, "modelProvider", MAX_FIELD_BYTES),
                thinking_level: opt_str(o, "thinkingLevel", MAX_FIELD_BYTES),
            });
        }
        Some(Self { request_id, agents, malformed, unknown_status })
    }
}

/// One dialog as listed by `agent_ui_list_response` (same shape as the live `agent_ui_request`
/// push). The payload is a bounded preview; the full payload is dropped, never stored or logged.
#[derive(Clone, Debug, PartialEq)]
pub struct WireUiRequest {
    pub request_id: String,
    pub agent_id: String,
    /// Open string: `select`, `confirm`, `input`, `editor`, `notify`, `setStatus`, …
    pub method: String,
    pub expects_response: bool,
    /// Truncated copy of the wire payload.
    pub payload: Value,
    /// Something was cut from `payload`.
    pub truncated: bool,
    pub surface_key: Option<String>,
    pub removed: bool,
    pub timeout_ms: Option<u64>,
    /// Epoch ms.
    pub created_at: i64,
}

impl WireUiRequest {
    /// A dialog the user can act on: it expects a response and is not a surface-clearing marker.
    #[must_use]
    pub fn is_attention(&self) -> bool {
        self.expects_response && !self.removed
    }

    /// `None` (malformed) without `requestId`, `agentId`, `method` or a usable `createdAt`.
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        let text = |k: &str| o.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(|s| cap_str(s, MAX_FIELD_BYTES).0);
        let (payload, truncated) = match o.get("payload") {
            Some(p) => preview(p, 0),
            None => (Value::Object(Map::new()), false),
        };
        Some(Self {
            request_id: text("requestId")?,
            agent_id: text("agentId")?,
            method: text("method")?,
            expects_response: o.get("expectsResponse").and_then(Value::as_bool).unwrap_or(false),
            payload,
            truncated,
            surface_key: opt_str(o, "surfaceKey", MAX_FIELD_BYTES),
            removed: o.get("removed").and_then(Value::as_bool).unwrap_or(false),
            timeout_ms: o.get("timeoutMs").and_then(Value::as_u64),
            created_at: o.get("createdAt").and_then(wire_time_ms)?,
        })
    }

    /// The preview's `payload.title` (string only).
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.payload.get("title").and_then(Value::as_str)
    }

    /// The preview's `payload.message` (string only).
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.payload.get("message").and_then(Value::as_str)
    }

    /// The preview's `payload.options` when it is a list; non-string entries are skipped.
    #[must_use]
    pub fn options(&self) -> Option<Vec<String>> {
        let list = self.payload.get("options")?.as_array()?;
        Some(list.iter().filter_map(|o| o.as_str().map(str::to_owned)).collect())
    }
}

/// Copy `v` with every string capped, every array/object limited and depth bounded.
fn preview(v: &Value, depth: usize) -> (Value, bool) {
    match v {
        Value::String(s) => {
            let (s, cut) = cap_str(s, MAX_PREVIEW_STRING_BYTES);
            (Value::String(s), cut)
        }
        Value::Array(a) => {
            if depth >= MAX_PREVIEW_DEPTH {
                return (Value::Null, true);
            }
            let mut cut = a.len() > MAX_PREVIEW_ENTRIES;
            let items = a
                .iter()
                .take(MAX_PREVIEW_ENTRIES)
                .map(|x| {
                    let (x, c) = preview(x, depth + 1);
                    cut |= c;
                    x
                })
                .collect();
            (Value::Array(items), cut)
        }
        Value::Object(m) => {
            if depth >= MAX_PREVIEW_DEPTH {
                return (Value::Null, true);
            }
            let mut cut = m.len() > MAX_PREVIEW_ENTRIES;
            let mut out = Map::new();
            for (k, x) in m.iter().take(MAX_PREVIEW_ENTRIES) {
                let (x, c) = preview(x, depth + 1);
                cut |= c;
                out.insert(cap_str(k, MAX_PREVIEW_STRING_BYTES).0, x);
            }
            (Value::Object(out), cut)
        }
        other => (other.clone(), false),
    }
}

/// `agent_ui_list_response {requestId, payload:{ok, pending[], surfaces[]?, error?}}`.
#[derive(Clone, Debug, PartialEq)]
pub struct UiListResponse {
    pub request_id: String,
    pub ok: bool,
    pub pending: Vec<WireUiRequest>,
    /// Pending entries dropped as malformed.
    pub malformed: u32,
    pub error: Option<String>,
}

impl UiListResponse {
    /// `None` without a string `requestId` or a boolean `payload.ok`. `surfaces[]` are
    /// non-dialog surfaces, never attention items: they are not decoded at all.
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        let request_id = m.get("requestId")?.as_str()?.to_owned();
        let payload = m.get("payload")?;
        let ok = payload.get("ok")?.as_bool()?;
        let (mut pending, mut malformed) = (Vec::new(), 0);
        if let Some(list) = payload.get("pending").and_then(Value::as_array) {
            for item in list {
                match WireUiRequest::from_value(item) {
                    Some(r) => pending.push(r),
                    None => malformed += 1,
                }
            }
        }
        Some(Self { request_id, ok, pending, malformed, error: opt_str(payload.as_object()?, "error", MAX_FIELD_BYTES) })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn status_mapping_round_trips() {
        for s in ["initializing", "idle", "running", "error", "closed"] {
            assert_eq!(AgentStatus::from_wire(s).as_wire(), s);
        }
        assert_eq!(AgentStatus::from_wire("hyperspace"), AgentStatus::Unknown);
    }

    #[test]
    fn preview_caps_strings_on_char_boundaries_and_lists() {
        let long = "é".repeat(3000); // 6000 bytes
        let options: Vec<String> = (0..500).map(|i| format!("o{i}")).collect();
        let v = json!({"requestId":"r","agentId":"a","method":"select","expectsResponse":true,"createdAt":1,
            "payload":{"title":"t","message":long,"options":options}});
        let r = WireUiRequest::from_value(&v).unwrap();
        assert!(r.truncated);
        assert!(r.message().unwrap().len() <= MAX_PREVIEW_STRING_BYTES);
        assert!(r.message().unwrap().chars().all(|c| c == 'é'));
        assert_eq!(r.options().unwrap().len(), MAX_PREVIEW_ENTRIES);
        assert_eq!(r.title(), Some("t"));
    }

    #[test]
    fn small_payloads_are_not_marked_truncated() {
        let v = json!({"requestId":"r","agentId":"a","method":"confirm","expectsResponse":true,"createdAt":"2026-01-01T00:00:00Z",
            "payload":{"title":"t","message":"m"}});
        let r = WireUiRequest::from_value(&v).unwrap();
        assert!(!r.truncated && r.created_at == 1_767_225_600_000 && r.is_attention());
    }

    #[test]
    fn deep_payloads_are_bounded() {
        let mut v = json!("leaf");
        for _ in 0..50 {
            v = json!({"x": v});
        }
        let (p, cut) = preview(&v, 0);
        assert!(cut);
        assert!(p.to_string().len() < 400);
    }

    #[test]
    fn dialog_requires_its_identity_fields() {
        let ok = json!({"requestId":"r","agentId":"a","method":"m","createdAt":1});
        assert!(WireUiRequest::from_value(&ok).is_some());
        for key in ["requestId", "agentId", "method", "createdAt"] {
            let mut bad = ok.clone();
            bad.as_object_mut().unwrap().remove(key);
            assert!(WireUiRequest::from_value(&bad).is_none(), "{key}");
        }
    }

    #[test]
    fn list_survives_bad_entries() {
        let m = json!({"requestId":"r","agents":[
            {"agentId":"a1","status":"hyperspace","title":42,"labels":{"k":"v","n":3,"deep":{"x":1}}},
            {"status":"idle"},
            "junk",
            {"agentId":"a2","status":"idle","title":"ok","lastActivity":"2026-01-01T00:00:00Z","cwd":7}
        ]});
        let r = ListAgentsResponse::from_message(&m).unwrap();
        assert_eq!((r.agents.len(), r.malformed, r.unknown_status), (2, 2, 1));
        assert_eq!((r.agents[0].status, r.agents[0].title.clone()), (AgentStatus::Unknown, None));
        assert_eq!(r.agents[0].labels.len(), 2);
        assert_eq!((r.agents[1].title.as_deref(), r.agents[1].cwd.clone(), r.agents[1].last_activity), (Some("ok"), None, Some(1_767_225_600_000)));
        assert!(ListAgentsResponse::from_message(&json!({"requestId":"r"})).is_none());
        assert!(ListAgentsResponse::from_message(&json!({"agents":[]})).is_none());
    }
}
