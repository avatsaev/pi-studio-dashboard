use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use pistudiodashboard_model::{MonitorState, ToastKind};
use serde_json::{Map, Value};

use crate::{EngineError, EngineServices, Result, commands, params};

/// A message for the user, produced by an effect; the shell moves it into its UI state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
}

/// One user's working state: the live monitor state, how often it changed, the injected hooks
/// and the toasts waiting for the shell. There is no history: live data is not an edit.
#[derive(Default)]
pub struct Session {
    /// Immutable. An update publishes a new `Arc`; readers keep the one they hold.
    monitor: Arc<MonitorState>,
    /// Grows with every published update. Caches key on it.
    revision: u64,
    pub(crate) services: EngineServices,
    pub(crate) pending_toasts: Vec<Toast>,
    pub(crate) daemon: crate::daemon_cmds::DaemonState,
}

impl Session {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A session over an existing monitor state (tests, fixtures, the fuzz seed).
    #[must_use]
    pub fn with_monitor(monitor: MonitorState) -> Self {
        Self { monitor: Arc::new(monitor), ..Self::default() }
    }

    #[must_use]
    pub fn with_services(mut self, services: EngineServices) -> Self {
        self.services = services;
        self
    }

    /// The current state, cheap to take and safe to keep: later updates don't change it.
    #[must_use]
    pub fn snapshot(&self) -> Arc<MonitorState> {
        Arc::clone(&self.monitor)
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn services(&self) -> &EngineServices {
        &self.services
    }

    /// Toasts queued by effects since the last call.
    pub fn take_toasts(&mut self) -> Vec<Toast> {
        std::mem::take(&mut self.pending_toasts)
    }

    /// Run a command by id. Never panics: unknown ids, wrong params and commands that cannot
    /// run now are errors, and a panic inside a command is caught and reported as one.
    ///
    /// # Errors
    /// Unknown id, non-object params, a disabled command, or the command's own error.
    pub fn execute(&mut self, id: &str, params: Value) -> Result<Value> {
        let spec = commands::find(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        let params = match params {
            Value::Null => Value::Object(Map::new()),
            Value::Object(_) => params,
            other => return Err(EngineError::BadParams(format!("`{id}` takes a JSON object, got {}", params::kind(&other)))),
        };
        (spec.enabled)(self).map_err(|reason| EngineError::Disabled { id: id.to_string(), reason })?;
        // Last-resort guard, not a licence to panic: `update` publishes only when its closure
        // returns `Ok`, so a command that unwinds leaves the state as it was.
        catch_unwind(AssertUnwindSafe(|| (spec.run)(self, &params))).unwrap_or_else(|_| {
            log::error!("command `{id}` panicked");
            Err(EngineError::Other(format!("`{id}` failed with an internal error (logged); the state is unchanged")))
        })
    }

    /// `f` changes a private copy of the state; only an `Ok` publishes it as the new snapshot
    /// and bumps the revision by one.
    ///
    /// # Errors
    /// Whatever `f` returns; the state is then unchanged.
    pub fn update<R>(&mut self, f: impl FnOnce(&mut MonitorState) -> Result<R>) -> Result<R> {
        let mut working = MonitorState::clone(&self.monitor);
        let out = f(&mut working)?;
        self.monitor = Arc::new(working);
        self.revision = self.revision.wrapping_add(1);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_failed_update_changes_nothing() {
        let mut s = Session::new();
        let before = s.snapshot();
        let r: Result<()> = s.update(|m| {
            m.dropped_events = 9;
            Err(EngineError::Other("no".into()))
        });
        assert!(r.is_err());
        assert!(Arc::ptr_eq(&before, &s.snapshot()));
        assert_eq!(s.revision(), 0);
        let ok: Result<()> = s.update(|m| {
            m.dropped_events = 1;
            Ok(())
        });
        assert!(ok.is_ok());
        assert_eq!(s.revision(), 1);
    }

    #[test]
    fn a_panic_in_a_command_is_an_error_and_the_state_survives() {
        let mut s = Session::new();
        let before = s.snapshot();
        let e = s.execute("test.panic", json!({})).unwrap_err();
        assert!(e.to_string().contains("internal error"), "{e}");
        assert!(Arc::ptr_eq(&before, &s.snapshot()));
        assert_eq!(s.revision(), 0);
        assert!(s.execute("command.list", json!({})).is_ok());
    }

    #[test]
    fn params_must_be_an_object_and_ids_must_exist() {
        let mut s = Session::new();
        assert!(matches!(s.execute("feed.ingest", json!([1, 2])), Err(EngineError::BadParams(m)) if m.contains("an array")));
        assert_eq!(s.execute("item.add", json!({})), Err(EngineError::UnknownCommand("item.add".into())));
    }

    #[test]
    fn snapshots_held_by_readers_do_not_change() {
        let mut s = Session::new();
        let held = s.snapshot();
        s.update(|m| {
            m.dropped_events = 5;
            Ok(())
        })
        .unwrap();
        assert_eq!(held.dropped_events, 0);
        assert_eq!(s.snapshot().dropped_events, 5);
    }
}
