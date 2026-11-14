//! Per-daemon slice: connection state, identity, health, counters and the event ring.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::caps::EVENT_RING;
use crate::event::ServerCaps;
use crate::ring::Ring;

/// Connection lifecycle of one daemon entry. Shared by the client (L4) and ingest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnState {
    #[default]
    Idle,
    Probing,
    Connecting,
    Handshaking,
    Syncing,
    Live,
    Backoff,
    AuthFailed,
    HostRejected,
    Disabled,
}

/// Capability gaps of a connected daemon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedReason {
    NoObservability,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ServerInfoView {
    pub hostname: Option<String>,
    pub version: Option<String>,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Health {
    pub reachable: bool,
    /// Epoch ms.
    pub at: i64,
}

/// Last connection error: a fixed-vocabulary code plus bounded text (never secrets).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ConnErrorView {
    pub code: String,
    pub message: Option<String>,
}

/// One unknown message type and how often it was seen (a daemon-chosen name, bounded).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UnknownTypeCount {
    pub name: String,
    pub count: u64,
}

/// Unknown types kept per daemon (the diagnostics panel lists the top ten).
pub const UNKNOWN_TYPES_SHOWN: usize = 10;

/// Diagnostics and ingest counters of one daemon (read by the diagnostics panel).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DaemonCounters {
    /// Unknown message types seen (sum of the client's per-type counts).
    pub unknown: u64,
    /// The top [`UNKNOWN_TYPES_SHOWN`] unknown types, most seen first.
    pub unknown_types: Vec<UnknownTypeCount>,
    pub malformed: u64,
    pub unknown_enum_values: u64,
    pub reconnect_count: u64,
    pub messages_total: u64,
    pub messages_per_second: u32,
    pub binary_frames_ignored: u64,
    pub frames_too_large: u64,
    pub dropped_events: u64,
    pub late_responses: u64,
    pub rtt_last_ms: Option<u64>,
    pub rtt_ewma_ms: Option<u64>,
    pub connected_since: Option<i64>,
}

impl ConnState {
    /// Fixed vocabulary for event summaries and logs.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Probing => "probing",
            Self::Connecting => "connecting",
            Self::Handshaking => "handshaking",
            Self::Syncing => "syncing",
            Self::Live => "live",
            Self::Backoff => "backoff",
            Self::AuthFailed => "auth_failed",
            Self::HostRejected => "host_rejected",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    DialogRaised,
    DialogResolved,
    TurnFailed,
    AgentError,
    ConnState,
    AgentRemoved,
}

/// One entry of the per-daemon event ring. `summary` is built from fixed vocabulary only:
/// never option text, message text, command or path content (D29, D32).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRecord {
    /// Epoch ms.
    pub at: i64,
    pub kind: EventKind,
    #[serde(default)]
    pub agent_id: Option<String>,
    pub summary: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DaemonSlice {
    pub entry_id: String,
    pub name: String,
    pub conn: ConnState,
    /// Newest connection epoch seen for this entry (D38).
    pub conn_epoch: u64,
    pub server_id: Option<String>,
    pub info: Option<ServerInfoView>,
    pub degraded: BTreeSet<DegradedReason>,
    pub duplicate_of: Option<String>,
    pub health: Option<Health>,
    /// Epoch ms of the first disconnect since the last rebuild.
    pub stale_since: Option<i64>,
    pub last_error: Option<ConnErrorView>,
    pub counters: DaemonCounters,
    /// Epoch ms the current `conn` state was entered.
    pub conn_since: Option<i64>,
    pub next_retry_in_ms: Option<u64>,
    pub caps: ServerCaps,
    pub tls_in_use: bool,
    pub is_loopback: bool,
    pub events: Ring<EventRecord, EVENT_RING>,
}

impl DaemonSlice {
    #[must_use]
    pub fn new(entry_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { entry_id: entry_id.into(), name: name.into(), ..Self::default() }
    }
}
