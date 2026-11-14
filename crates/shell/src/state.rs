//! Everything the shell remembers that is not the monitor state. Plain data: `ui.inspect`
//! returns it, `ui.set` changes it, the frontend draws it and tests compare it. What a widget
//! needs only while it is on screen (an open menu, text typed but not submitted) stays in the
//! frontend.
//!
//! Decision: `UiState` is the runtime source of truth for `theme` and `attentionRailCollapsed`;
//! `MonitorState.settings` mirrors notification and tray settings, and the config file is
//! written from them (sprint-005/task-002). `theme` carries the user's *choice* (`system`
//! included); the webview resolves `system` against the OS, never this crate.

use pistudiodashboard_engine::GroupBy;
use pistudiodashboard_model::{AgentStatus, ToastKind};
use serde::{Deserialize, Serialize};

/// Toasts kept on screen; the oldest is dropped beyond this.
const MAX_TOASTS: usize = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeKind {
    #[default]
    Dark,
    Light,
    System,
}

impl ThemeKind {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "dark" => Some(ThemeKind::Dark),
            "light" => Some(ThemeKind::Light),
            "system" => Some(ThemeKind::System),
            _ => None,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ThemeKind::Dark => "dark",
            ThemeKind::Light => "light",
            ThemeKind::System => "system",
        }
    }
}

/// One toast as the frontend draws it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToastView {
    pub id: u64,
    pub kind: ToastKind,
    pub text: String,
}

/// The attention item the window is focused on (set by `window.focusItem`, sprint-008).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusItem {
    pub server_id: String,
    pub item_id: String,
}

/// The visible slice of the card grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridWindow {
    pub start: u32,
    pub count: u32,
}

impl Default for GridWindow {
    fn default() -> Self {
        Self { start: 0, count: 100 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormMode {
    Add,
    Edit,
}

/// An open dialog. Only open/close state lives here; typed-but-unsubmitted text does not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Dialog {
    DaemonForm {
        mode: FormMode,
        #[serde(default)]
        entry_id: Option<String>,
    },
    Password {
        entry_id: String,
    },
    ConfirmInterrupt {
        server_id: String,
        agent_id: String,
    },
    RemoveDaemon {
        entry_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiState {
    pub theme: ThemeKind,
    /// serverIds; empty = all daemons.
    pub filter_daemons: Vec<String>,
    /// Empty = all statuses.
    pub filter_statuses: Vec<AgentStatus>,
    pub attention_rail_collapsed: bool,
    /// How the card grid is laid out; saved in the config file (`settings.groupBy`).
    pub group_by: GroupBy,
    /// Zoom of the window's content in percent; saved in the config file (`settings.zoom`).
    pub zoom: u16,
    pub focus_item: Option<FocusItem>,
    pub grid_window: GridWindow,
    pub dialog: Option<Dialog>,
    pub diagnostics_target: Option<String>,
    /// At most five, oldest first; read-only through `ui.set`.
    pub toasts: Vec<ToastView>,
    /// The last command's error, shown in the status bar. Empty after a success.
    pub status: String,
    #[serde(skip)]
    next_toast_id: u64,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            theme: ThemeKind::default(),
            filter_daemons: Vec::new(),
            filter_statuses: Vec::new(),
            attention_rail_collapsed: false,
            group_by: GroupBy::default(),
            zoom: crate::zoom::DEFAULT,
            focus_item: None,
            grid_window: GridWindow::default(),
            dialog: None,
            diagnostics_target: None,
            toasts: Vec::new(),
            status: String::new(),
            next_toast_id: 0,
        }
    }
}

impl UiState {
    /// Add a toast, dropping the oldest beyond the bound.
    pub fn push_toast(&mut self, kind: ToastKind, text: String) {
        self.next_toast_id = self.next_toast_id.wrapping_add(1);
        self.toasts.push(ToastView { id: self.next_toast_id, kind, text });
        if self.toasts.len() > MAX_TOASTS {
            let excess = self.toasts.len() - MAX_TOASTS;
            self.toasts.drain(..excess);
        }
    }
}
