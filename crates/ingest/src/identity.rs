//! Daemon identity (D19): the first `Hello` that carries a `server_id` owns it; later entries
//! with the same id become duplicates and own no agents or dialogs.

use pistudiodashboard_model::{AgentEntry, Effect, MonitorState, ToastKind, UiRequestItem};

use crate::keys::take_server;

const REPLACED: &str = "daemon replaced";

fn agent_of(a: &AgentEntry, sid: &str) -> bool {
    a.server_id == sid
}

fn dialog_of(u: &UiRequestItem, sid: &str) -> bool {
    u.server_id == sid
}

/// Drop every agent and dialog of `server_id`.
pub(crate) fn remove_server(state: &mut MonitorState, server_id: &str) {
    drop(take_server(&mut state.agents, server_id, agent_of));
    drop(take_server(&mut state.ui_requests, server_id, dialog_of));
}

/// Record `sid` as the identity of slice `idx`.
///
/// * A changed id at an owning entry removes the old id's agents, dialogs and events and closes
///   a drill-down on it (`CloseDrill`, toast "daemon replaced").
/// * Another owning entry with the same id makes this slice its duplicate (`duplicate_of`).
///
/// Known limit: when an owner changes id, an existing duplicate of the old id is re-resolved at
/// its next `Hello` (reconnect), not immediately.
pub(crate) fn resolve_identity(state: &mut MonitorState, idx: usize, sid: &str, effects: &mut Vec<Effect>) {
    let Some(slice) = state.daemons.get(idx) else { return };
    let old = slice.server_id.clone();
    let was_owner = slice.duplicate_of.is_none();
    if let Some(old) = old.filter(|o| o != sid && was_owner) {
        replace_identity(state, idx, &old, effects);
    }
    let owner = state
        .daemons
        .iter()
        .enumerate()
        .find(|(i, d)| *i != idx && d.server_id.as_deref() == Some(sid) && d.duplicate_of.is_none())
        .map(|(_, d)| d.entry_id.clone());
    if let Some(slice) = state.daemons.get_mut(idx) {
        slice.duplicate_of = owner;
        slice.server_id = Some(sid.to_owned());
    }
}

fn replace_identity(state: &mut MonitorState, idx: usize, old: &str, effects: &mut Vec<Effect>) {
    remove_server(state, old);
    if let Some(slice) = state.daemons.get_mut(idx) {
        slice.events.clear();
        slice.stale_since = None;
    }
    if state.drilldown.as_ref().is_some_and(|d| d.target.server_id == old) {
        state.drilldown = None;
        effects.push(Effect::Toast { kind: ToastKind::Info, text: REPLACED.to_owned() });
        effects.push(Effect::CloseDrill { reason: REPLACED.to_owned() });
    }
}
