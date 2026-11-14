//! Drill-down buffer for the selected agent (D6, D17, D27).

use serde::{Deserialize, Serialize};

use pistudiodashboard_protocol::StreamEvent;

use crate::caps::DRILL_ITEMS;
use crate::ring::Ring;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DrillTarget {
    pub server_id: String,
    pub agent_id: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DrillState {
    #[default]
    Loading,
    Live,
    Refetching,
}

/// Turn boundary kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnKind {
    Started,
    Completed,
    Failed,
    Canceled,
}

/// Internally tagged by `type` because `tool` carries a `kind` field of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DrillItem {
    Assistant {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Tool {
        call_id: String,
        kind: String,
        label: String,
        status: String,
        /// Last `TOOL_OUTPUT_TAIL_LINES` lines, each <= `TOOL_OUTPUT_LINE`.
        #[serde(default)]
        output_tail: Option<Vec<String>>,
    },
    Error {
        message: String,
    },
    Turn {
        kind: TurnKind,
    },
    System {
        text: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DrillBuffer {
    /// May name an agent absent from `agents` (stale/removed); reducers close the drill-down
    /// on removal, so `validate` does not require existence.
    pub target: DrillTarget,
    pub items: Ring<DrillItem, DRILL_ITEMS>,
    pub seq_high: Option<u64>,
    pub state: DrillState,
    pub truncated_at_start: bool,
    pub stale: bool,
    /// Live stream events received while `loading` / `refetching` (<= `DRILL_HOLD`), replayed
    /// after the timeline arrives.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<HeldEvent>,
    /// The text block deltas append to; `None` after `final` or any other item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_block: Option<BlockKind>,
}

/// The kind of text block a stream of deltas is currently appending to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockKind {
    Assistant,
    Reasoning,
}

/// A stream event waiting for the timeline load.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldEvent {
    #[serde(default)]
    pub seq: Option<u64>,
    pub event: StreamEvent,
}
