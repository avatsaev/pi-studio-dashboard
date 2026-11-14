//! Pi dialogs (`agent_ui_request` with `expectsResponse`): first-class attention items (D36).
//! Construction from the wire (`from_wire`) lives with the ingest reducers.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestKind {
    Confirm,
    Select,
    Input,
    Editor,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Answerability {
    Answerable,
    #[default]
    HandOff,
}

/// Placeholder/prefill/editor text is never retained (D29/D32).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiRequestItem {
    pub server_id: String,
    pub request_id: String,
    pub agent_id: String,
    pub method: String,
    pub kind: RequestKind,
    pub answerability: Answerability,
    /// <= `DIALOG_TITLE`.
    pub title: Option<String>,
    /// <= `DIALOG_MESSAGE` (confirm).
    pub message: Option<String>,
    /// <= `DIALOG_OPTIONS` entries, each <= `DIALOG_OPTION` (select).
    pub options: Option<Vec<String>>,
    pub options_truncated: bool,
    pub timeout_ms: Option<u64>,
    /// Epoch ms, from the wire.
    pub created_at: i64,
    /// Epoch ms, local, derived from `created_at + timeout_ms`.
    pub expires_at: Option<i64>,
    /// Epoch ms, local receipt time.
    pub raised_at: i64,
    pub in_flight: bool,
    /// The answer sent while `in_flight` (bounded like an option).
    pub in_flight_response: Option<String>,
    /// Set when the owning daemon disconnected: answer controls are disabled.
    pub stale: bool,
}
