//! Agent lifecycle pushes: `agent_update`, `agent_status`, `agent_archived`, `agent_deleted`,
//! `agent_timeline_reset`. All lenient; only `agentId` is required.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::lenient::{cap_str, wire_time_ms};
use crate::sync::AgentStatus;

const MAX_TITLE_BYTES: usize = 256;
const MAX_FIELD_BYTES: usize = 512;

fn agent_id(m: &Value) -> Option<String> {
    m.get("agentId")?.as_str().filter(|s| !s.is_empty()).map(|s| cap_str(s, MAX_FIELD_BYTES).0)
}

fn opt_str(o: &Map<String, Value>, k: &str, max: usize) -> Option<String> {
    o.get(k).and_then(Value::as_str).map(|s| cap_str(s, max).0)
}

/// `agent_update`: a partial patch. Absent fields mean "unchanged"; `title: null` clears.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUpdate {
    pub agent_id: String,
    /// The raw wire status; map with [`AgentStatus::from_wire`].
    pub status: Option<String>,
    /// `None` = absent, `Some(None)` = `null` (clear), `Some(Some(t))` = set.
    pub title: Option<Option<String>>,
    /// `Some` = the daemon sent a `labels` object (scalar values only).
    pub labels: Option<BTreeMap<String, String>>,
    pub model: Option<String>,
    pub model_provider: Option<String>,
}

impl AgentUpdate {
    /// `None` (malformed) without `agentId`.
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        let id = agent_id(m)?;
        let o = m.as_object()?;
        let title = match o.get("title") {
            Some(Value::Null) => Some(None),
            Some(Value::String(s)) => Some(Some(cap_str(s, MAX_TITLE_BYTES).0)),
            _ => None,
        };
        let labels = o.get("labels").and_then(Value::as_object).map(|l| {
            l.iter()
                .filter_map(|(k, v)| {
                    let t = match v {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => return None,
                    };
                    Some((cap_str(k, MAX_FIELD_BYTES).0, cap_str(&t, MAX_FIELD_BYTES).0))
                })
                .take(100)
                .collect()
        });
        Some(Self {
            agent_id: id,
            status: opt_str(o, "status", MAX_FIELD_BYTES),
            title,
            labels,
            model: opt_str(o, "model", MAX_FIELD_BYTES),
            model_provider: opt_str(o, "modelProvider", MAX_FIELD_BYTES),
        })
    }

    /// The status carries a value this build does not know.
    #[must_use]
    pub fn has_unknown_status(&self) -> bool {
        self.status.as_deref().is_some_and(|s| AgentStatus::from_wire(s) == AgentStatus::Unknown)
    }
}

/// `agent_status {agentId, status}`: never emitted by any daemon path; decoded for tolerance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStatusMsg {
    pub agent_id: String,
    pub status: String,
}

impl AgentStatusMsg {
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        Some(Self { agent_id: agent_id(m)?, status: opt_str(m.as_object()?, "status", MAX_FIELD_BYTES)? })
    }
}

/// `agent_archived {agentId, archivedAt?}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentArchived {
    pub agent_id: String,
    /// Epoch ms; `None` when absent or unparsable.
    pub archived_at: Option<i64>,
}

impl AgentArchived {
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        Some(Self { agent_id: agent_id(m)?, archived_at: m.get("archivedAt").and_then(wire_time_ms) })
    }
}

/// `agent_deleted {agentId}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentDeleted {
    pub agent_id: String,
}

impl AgentDeleted {
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        Some(Self { agent_id: agent_id(m)? })
    }
}

/// `agent_timeline_reset {agentId, reason?}` (open string; shipped value `"fork"`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineReset {
    pub agent_id: String,
    pub reason: Option<String>,
}

impl TimelineReset {
    #[must_use]
    pub fn from_message(m: &Value) -> Option<Self> {
        Some(Self { agent_id: agent_id(m)?, reason: opt_str(m.as_object()?, "reason", MAX_FIELD_BYTES) })
    }
}
