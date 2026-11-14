//! The shell: everything a frontend needs that is not the engine, with no UI toolkit in it.
//!
//! Every action goes through [`AppShell::run`] (and from there
//! [`pistudiodashboard_engine::Session::execute`]), all UI state is plain data in [`state::UiState`], and
//! [`control::view`] is the one value a frontend draws: the UI state, the document, and the menu
//! bar generated from the command registry. The webview in `apps/pistudiodashboard` renders that value and
//! sends ids back; automation reads and drives the same thing through [`control::handle`].
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod control;
mod fields;
pub mod menus;
pub mod services;
pub mod shortcuts;
pub mod state;
pub mod zoom;

use pistudiodashboard_engine::{GroupBy, Session};
use pistudiodashboard_model::ConnState;
use serde_json::Value;
use state::GridWindow;

pub use control::{Envelope, Request};
pub use services::{LocalDaemon, Services};
pub use state::{Dialog, ThemeKind, ToastView, UiState};

pub struct AppShell {
    pub session: Session,
    pub ui: UiState,
    pub services: Services,
    /// Entries whose password prompt waits for the open dialog to close.
    waiting_prompts: Vec<String>,
}

impl AppShell {
    pub fn new(session: Session, services: Services) -> Self {
        Self { session, ui: UiState::default(), services, waiting_prompts: Vec::new() }
    }

    /// Set the grouping mode: the UI state, the saved setting (the engine saves only on a change)
    /// and, on a change, the grid window back to the top (the layout under it just changed).
    pub fn set_group_by(&mut self, group_by: GroupBy) {
        if self.ui.group_by != group_by {
            self.ui.group_by = group_by;
            self.ui.grid_window = GridWindow::default();
        }
        self.session.set_group_by(group_by);
    }

    /// Set the zoom (percent): the UI state, which the app applies to the window, and the saved setting.
    pub fn set_zoom(&mut self, zoom: u16) {
        self.ui.zoom = zoom;
        self.session.set_zoom(zoom);
    }

    /// Take the saved settings the UI state mirrors; call once after the config is loaded.
    pub fn adopt_settings(&mut self) {
        let settings = &self.session.config().settings;
        self.ui.group_by = settings.group_by;
        self.ui.zoom = settings.zoom;
    }

    /// Run a command by id. Every click, shortcut, menu item and control request ends here:
    /// shell commands (pure view state) first, then the engine. The outcome goes to the
    /// status bar as well as to the caller.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        let result = match menus::shell_command(id) {
            Some(command) => (command.enabled)(self).and_then(|()| (command.run)(self, &params)).map(|()| Value::Null),
            None => self.session.execute(id, params).map_err(|e| e.to_string()),
        };
        self.ui.status = result.as_ref().err().cloned().unwrap_or_default();
        self.settle();
        result
    }

    /// An engine command from the app's own plumbing: like [`run`](Self::run) but the outcome
    /// does not touch the status bar (a background batch must not clear a user's error).
    pub fn run_internal(&mut self, id: &str, params: Value) -> Result<Value, String> {
        let result = self.session.execute(id, params).map_err(|e| e.to_string());
        self.settle();
        result
    }

    /// After any command: move toasts into the UI state, open password prompts, and close UI
    /// that points at a daemon entry that no longer exists.
    fn settle(&mut self) {
        for toast in self.session.take_toasts() {
            self.ui.push_toast(toast.kind, toast.text);
        }
        self.waiting_prompts.extend(self.session.take_prompts().into_iter().map(|p| p.entry_id));
        let snapshot = self.session.snapshot();
        let exists = |id: &str| snapshot.daemon(id).is_some();
        if self.ui.diagnostics_target.as_deref().is_some_and(|id| !exists(id)) {
            self.ui.diagnostics_target = None;
        }
        let stale = match &self.ui.dialog {
            Some(Dialog::Password { entry_id } | Dialog::RemoveDaemon { entry_id }) => !exists(entry_id),
            Some(Dialog::DaemonForm { entry_id: Some(entry_id), .. }) => !exists(entry_id),
            _ => false,
        };
        if stale {
            self.ui.dialog = None;
        }
        self.waiting_prompts.retain(|id| snapshot.daemon(id).is_some_and(|d| d.conn == ConnState::AuthFailed));
        if self.ui.dialog.is_none() && !self.waiting_prompts.is_empty() {
            let entry_id = self.waiting_prompts.remove(0);
            self.ui.dialog = Some(Dialog::Password { entry_id });
        }
    }

    /// Answer one request from the frontend or from automation: `{"ok": true, "result": …}`
    /// or `{"ok": false, "error": "…"}`.
    pub fn answer(&mut self, request: &Request) -> Value {
        match request {
            Request::Control { method, params } => control::handle(self, method, params),
            Request::Invoke { command } => control::reply(menus::invoke(self, command)),
            Request::Internal { command, params } => control::reply(self.run_internal(command, params.clone())),
        }
    }
}

#[cfg(test)]
mod monitor_tests;
#[cfg(test)]
mod tests;
