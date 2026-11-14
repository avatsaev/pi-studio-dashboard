//! Side effects requested by reducers; the owner thread executes them (reducers stay pure).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotifyKind {
    NeedsInput,
    Failures,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToastKind {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Effect {
    /// `item_id` is `"ui:<requestId>"` or `"fail:<agentId>"`.
    Notify {
        kind: NotifyKind,
        server_id: String,
        item_id: String,
    },
    ResolveNotification {
        server_id: String,
        item_id: String,
    },
    Toast {
        kind: ToastKind,
        text: String,
    },
    CloseDrill {
        reason: String,
    },
    FetchStats {
        server_id: String,
        agent_id: String,
    },
    /// Ask the daemon for its agent list. `agent_update` carries no `cwd`, provider or model, so an
    /// agent first seen through a push (created after the connect-time snapshot) is a placeholder
    /// until the answer (`AgentsListed`) fills it in.
    FetchAgents {
        server_id: String,
    },
    /// One logical (re)load; the client does the paging (D39).
    FetchTimeline {
        server_id: String,
        agent_id: String,
    },
}
