//! Live monitor state (L1): the plain-serde `MonitorState`, its component types, the bounded
//! [`Ring`] and every cap from the live-state spec.
//!
//! No I/O and no clock reads (`now` is always an argument of later reducers); wasm-clean.
//! Reducers live in the ingest crate; this crate owns the data and the invariants
//! ([`MonitorState::validate`]).
//!
//! Map key strategy: JSON objects only allow string keys, so `agents` and `ui_requests` are
//! `BTreeMap<String, _>` keyed by `"<serverId>\u{1f}<second id>"` (see [`agent_key`] and
//! [`ui_request_key`]); lookups are O(log n). `daemons` is a `Vec` in config order (few entries,
//! unique `entry_id`).
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod agent;
pub mod caps;
pub mod daemon;
pub mod dialog;
pub mod drill;
pub mod effect;
pub mod event;
pub mod ring;
pub mod settings;
pub mod state;

pub use agent::{ActivitySummary, AgentEntry, AgentStatus, Attention, ContextUsage, CurrentTool, TokenUsage, TurnState, UsageSnapshot};
pub use caps::*;
pub use daemon::{
    ConnErrorView, ConnState, DaemonCounters, DaemonSlice, DegradedReason, EventKind, EventRecord, Health, ServerInfoView, UNKNOWN_TYPES_SHOWN,
    UnknownTypeCount,
};
pub use dialog::{Answerability, RequestKind, UiRequestItem};
pub use drill::{BlockKind, DrillBuffer, DrillItem, DrillState, DrillTarget, HeldEvent, TurnKind};
pub use effect::{Effect, NotifyKind, ToastKind};
pub use event::{
    AgentListItem, CountersSnapshot, DaemonEvent, DaemonEventKind, MutationOp, MutationResult, PendingUi, ServerCaps, StatsResult, StreamEvent, TimelineItem,
    ToolInfo,
};
pub use ring::Ring;
pub use settings::{NotificationPrefs, SettingsMirror, Theme};
pub use state::{Invalid, MonitorState, Observability, agent_key, ui_request_key};
