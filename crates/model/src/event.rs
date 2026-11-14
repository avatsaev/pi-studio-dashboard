//! Input of the ingest reducers: what the client (L4) emits per daemon. Model-owned plain
//! structs so L2 does not depend on typed protocol messages; the client maps wire -> these.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::agent::{ContextUsage, TokenUsage};
use crate::daemon::{ConnErrorView, ConnState, DegradedReason, ServerInfoView};

pub use pistudiodashboard_protocol::{StreamEvent, TimelineItem, ToolInfo};

/// Capabilities derived from `server_info.features` (only literal `true` counts).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ServerCaps {
    pub daemon_observability: bool,
}

/// One agent of `list_agents_response`. `labels` are already flattened by the client to
/// short strings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentListItem {
    pub agent_id: String,
    /// Raw wire status; mapped with `AgentStatus::from_wire`.
    pub status: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub labels: Vec<String>,
    /// Epoch ms.
    pub last_activity: Option<i64>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub thinking_level: Option<String>,
}

/// One pending dialog (`agent_ui_request`-shaped). The client extracts `title`, `message` and
/// `options` from the wire payload; no other payload content crosses this boundary (D29/D32).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PendingUi {
    pub request_id: String,
    pub agent_id: String,
    pub method: String,
    pub expects_response: bool,
    pub removed: bool,
    pub title: Option<String>,
    pub message: Option<String>,
    pub options: Option<Vec<String>>,
    pub timeout_ms: Option<u64>,
    /// Epoch ms, from the wire.
    pub created_at: i64,
}

/// Counters snapshot of one connection (protocol-client § Diagnostics counters).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CountersSnapshot {
    pub reconnect_count: u64,
    pub messages_total: u64,
    pub messages_per_second: u32,
    /// Bounded by the client (<= 64 keys).
    pub unknown_types: BTreeMap<String, u64>,
    pub unknown_enum_values: u64,
    pub malformed: u64,
    pub binary_frames_ignored: u64,
    pub frames_too_large: u64,
    pub dropped_events: u64,
    pub late_responses: u64,
    pub rtt_last_ms: Option<u64>,
    pub rtt_ewma_ms: Option<u64>,
    /// Epoch ms.
    pub connected_since: Option<i64>,
}

/// Which mutation a `RequestResult` is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MutationOp {
    RespondUi,
    InterruptAgent,
}

/// Result of a mutation. `NotFound` / `Rejected` / `Interrupt{ok:false}` are domain results from
/// the daemon; the rest are transport outcomes (nothing is ever queued or replayed, D27).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MutationResult {
    /// `RespondUi` accepted.
    Ok,
    /// `RespondUi`: `ok:false, error:"not_found"` (unknown or already resolved).
    NotFound,
    /// `RespondUi`: `ok:false` with another error code.
    Rejected {
        error: String,
    },
    /// `InterruptAgent`: the daemon's `ok` flag.
    Interrupt {
        ok: bool,
    },
    /// The daemon answered with `rpc_error`.
    RpcError {
        code: Option<String>,
        message: Option<String>,
    },
    NotConnected,
    Timeout,
    ConnectionLost,
}

/// `Option<Option<T>>` as "absent / null / value" (a partial patch field that can be cleared).
mod patch {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[allow(clippy::option_option)]
    pub fn serialize<S: Serializer>(v: &Option<Option<String>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(inner) => inner.serialize(s),
            None => s.serialize_none(),
        }
    }

    #[allow(clippy::option_option)]
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
        Option::<String>::deserialize(d).map(Some)
    }
}

/// Usage carried by a `Stats` event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum StatsResult {
    /// The agent has no live session (`ok:false`): shown as "—".
    NoSession,
    Usage {
        #[serde(default)]
        tokens: Option<TokenUsage>,
        #[serde(default)]
        cost: Option<f64>,
        #[serde(default)]
        context_usage: Option<ContextUsage>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DaemonEventKind {
    ConnState {
        state: ConnState,
        /// Epoch ms the state was entered.
        since: i64,
        last_error: Option<ConnErrorView>,
        next_retry_in_ms: Option<u64>,
    },
    Hello {
        info: ServerInfoView,
        caps: ServerCaps,
        degraded: BTreeSet<DegradedReason>,
        tls_in_use: bool,
        is_loopback: bool,
    },
    Health {
        reachable: bool,
        at: i64,
    },
    Snapshot {
        agents: Vec<AgentListItem>,
        ui_pending: Vec<PendingUi>,
    },
    Rtt {
        ms: u64,
    },
    Diagnostics(CountersSnapshot),
    /// Outcome of a mutation (`RespondUi` / `InterruptAgent`); `token` is the caller's opaque key.
    /// Reducer handling arrives with sprint-007/task-002.
    RequestResult {
        token: String,
        op: MutationOp,
        result: MutationResult,
    },
    /// A pushed `agent_ui_resolved`. The client forwards it after the `Snapshot` it raced with,
    /// so a resolved dialog is not resurrected by an older list (D27, D36). Reducer: sprint-007.
    UiResolved {
        request_id: String,
        agent_id: String,
        reason: String,
    },
    /// `agent_update`: a partial patch. Absent fields are unchanged; `title: null` clears it.
    AgentUpdate {
        agent_id: String,
        /// Raw wire status, mapped with `AgentStatus::from_wire`.
        #[serde(default)]
        status: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", with = "patch")]
        #[allow(clippy::option_option)]
        title: Option<Option<String>>,
        /// Flattened `key=value` labels; `Some` replaces the agent's labels.
        #[serde(default)]
        labels: Option<Vec<String>>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        model_provider: Option<String>,
    },
    /// `agent_status`: decoded for tolerance; no daemon path emits it.
    AgentStatus {
        agent_id: String,
        status: String,
    },
    AgentArchived {
        agent_id: String,
        /// Epoch ms.
        #[serde(default)]
        archived_at: Option<i64>,
    },
    AgentDeleted {
        agent_id: String,
    },
    /// A stream event of an agent. `seq` is absent for ephemeral events (partial tool calls, deltas).
    AgentStream {
        agent_id: String,
        #[serde(default)]
        seq: Option<u64>,
        /// Epoch ms.
        #[serde(default)]
        timestamp: Option<i64>,
        event: StreamEvent,
    },
    /// `agent_timeline_reset`: the daemon rewrote the agent's timeline (e.g. a fork).
    TimelineReset {
        agent_id: String,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Result of a `SessionStats` request (the client executes, the engine schedules, D39).
    Stats {
        agent_id: String,
        result: StatsResult,
    },
    /// One page of a timeline load: `reset` replaces the drill buffer, later pages append.
    Timeline {
        agent_id: String,
        items: Vec<TimelineItem>,
        reset: bool,
        seq_end: u64,
    },
}

/// `server_id` is the daemon's identity once known (`Hello` carries it; the client tags later
/// events with it). `conn_epoch` increments per connection attempt (D38).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonEvent {
    pub entry_id: String,
    #[serde(default)]
    pub server_id: Option<String>,
    pub conn_epoch: u64,
    pub seq: u64,
    /// Epoch ms.
    pub at: i64,
    pub kind: DaemonEventKind,
}
