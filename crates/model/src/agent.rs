//! Agent cards: status, activity summary, usage and the attention cache.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Initializing,
    Idle,
    Running,
    Error,
    Closed,
    /// Any wire value this build does not know (D26), and the default.
    #[default]
    #[serde(other)]
    Unknown,
}

impl AgentStatus {
    /// Lenient wire mapping: unknown strings become [`AgentStatus::Unknown`].
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
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnState {
    #[default]
    None,
    Started,
    Completed,
    Failed,
    Canceled,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CurrentTool {
    pub call_id: Option<String>,
    pub kind: String,
    /// Single line, <= `LABEL`.
    pub label: String,
    pub status: String,
}

/// Derived only (D17); never holds raw output beyond one capped line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ActivitySummary {
    pub current_tool: Option<CurrentTool>,
    pub last_output_line: Option<String>,
    pub turn_state: TurnState,
    pub last_error_message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TokenUsage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub total: Option<u64>,
}

/// Context-window usage as reported by the daemon (`contextUsage {tokens, contextWindow, percent}`).
/// TODO(verify): the mock daemon's stats payload is `{sessionId, totalMessages, tokens:{total}, model}`
/// with no `contextUsage` and no `cost`; the field set is unverified against a real provider.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextUsage {
    pub tokens: Option<u64>,
    pub window: Option<u64>,
    pub percent: Option<f64>,
}

/// `None` renders as "—" (D16).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UsageSnapshot {
    #[default]
    None,
    Some {
        #[serde(default)]
        tokens: Option<TokenUsage>,
        #[serde(default)]
        cost: Option<f64>,
        #[serde(default)]
        context_usage: Option<ContextUsage>,
        /// Epoch ms the snapshot was taken ("as of").
        taken_at: i64,
    },
}

/// Reducer-maintained cache.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Attention {
    pub pending_dialogs: u32,
    pub errored: bool,
}

/// One agent card. Strings stay plain `String` (budget: <= ~2 KiB per entry).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentEntry {
    pub server_id: String,
    pub agent_id: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub labels: Vec<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub status: AgentStatus,
    /// Epoch ms.
    pub last_activity: Option<i64>,
    pub archived: bool,
    pub activity: ActivitySummary,
    pub usage: UsageSnapshot,
    pub attention: Attention,
    pub stale: bool,
    /// Reducer buffer: the tail of the assistant text of the open message (<= `ASSISTANT_TAIL`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub assistant_tail: String,
}
