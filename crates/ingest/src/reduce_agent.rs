//! Agent reducers: update, status, archived, deleted and stream events.

use pistudiodashboard_model::{
    AgentEntry, AgentListItem, AgentStatus, Attention, Effect, EventKind, EventRecord, LABEL, MonitorState, NotifyKind, SUMMARY_TITLE, StreamEvent, agent_key,
    cap_string,
};

use crate::activity::update_activity;
use crate::apply::Ctx;
use crate::drill;

/// The daemon identity events of slice `ctx.idx` belong to. A slice without a `server_id` cannot
/// key anything (the event is dropped and counted); a duplicate slice owns nothing (ignored).
pub(crate) fn owner(state: &mut MonitorState, ctx: Ctx) -> Option<String> {
    let slice = state.daemons.get(ctx.idx)?;
    match (&slice.server_id, &slice.duplicate_of) {
        (Some(sid), None) => Some(sid.clone()),
        (None, _) => {
            state.dropped_events = state.dropped_events.saturating_add(1);
            None
        }
        (Some(_), Some(_)) => None,
    }
}

fn push_event(state: &mut MonitorState, ctx: Ctx, kind: EventKind, agent_id: &str, summary: &str) {
    if let Some(slice) = state.daemons.get_mut(ctx.idx) {
        slice.events.push(EventRecord { at: ctx.at, kind, agent_id: Some(agent_id.to_owned()), summary: summary.to_owned() });
    }
}

fn fail_notification(sid: &str, agent_id: &str) -> Effect {
    Effect::Notify { kind: NotifyKind::Failures, server_id: sid.to_owned(), item_id: format!("fail:{agent_id}") }
}

/// A patch of an agent card; `None` fields are unchanged.
pub(crate) struct Patch<'a> {
    pub status: Option<&'a str>,
    pub title: Option<Option<&'a str>>,
    pub labels: Option<&'a [String]>,
    pub model: Option<&'a str>,
}

/// `AgentUpdate` / `AgentStatus`: create a placeholder for an unknown agent (and ask for its
/// stats), patch the present fields, then run the status-change rules.
pub(crate) fn update(state: &mut MonitorState, ctx: Ctx, agent_id: &str, patch: &Patch<'_>, effects: &mut Vec<Effect>) {
    let Some(sid) = owner(state, ctx) else { return };
    let key = agent_key(&sid, agent_id);
    if !state.agents.contains_key(&key) {
        state.agents.insert(
            key.clone(),
            AgentEntry {
                server_id: sid.clone(),
                agent_id: agent_id.to_owned(),
                status: AgentStatus::Idle,
                attention: Attention::default(),
                ..AgentEntry::default()
            },
        );
        effects.push(Effect::FetchStats { server_id: sid.clone(), agent_id: agent_id.to_owned() });
        // A push never says where the agent lives; only the list does (the card would stay in "No workspace").
        effects.push(Effect::FetchAgents { server_id: sid.clone() });
    }
    let Some(entry) = state.agents.get_mut(&key) else { return };
    let (old_status, old_title) = (entry.status, entry.title.clone());
    if let Some(s) = patch.status {
        entry.status = AgentStatus::from_wire(s);
    }
    match patch.title {
        Some(Some(t)) => entry.title = Some(cap_string(t, SUMMARY_TITLE)),
        Some(None) => entry.title = None,
        None => {}
    }
    if let Some(labels) = patch.labels {
        entry.labels = labels.iter().map(|l| cap_string(l, LABEL)).collect();
    }
    if let Some(m) = patch.model {
        entry.model = Some(cap_string(m, LABEL));
    }
    if entry.status != old_status || entry.title != old_title {
        entry.last_activity = Some(ctx.now);
    }
    let new_status = entry.status;
    if old_status == AgentStatus::Running && new_status != AgentStatus::Running {
        entry.activity.current_tool = None;
    }
    if new_status == AgentStatus::Error && old_status != AgentStatus::Error {
        entry.attention.errored = true;
        push_event(state, ctx, EventKind::AgentError, agent_id, "agent error");
        effects.push(fail_notification(&sid, agent_id));
    } else if old_status == AgentStatus::Error && new_status != AgentStatus::Error {
        entry.attention.errored = false;
    }
}

/// `AgentsListed`: fill what pushes never carry (`cwd`, provider) into agents already known, and
/// title/model only where nothing is known yet (a push since the list was built is newer). An
/// agent the state does not know is ignored: the list must not resurrect a deleted or archived one.
pub(crate) fn listed(state: &mut MonitorState, ctx: Ctx, items: &[AgentListItem]) {
    let Some(sid) = owner(state, ctx) else { return };
    for item in items {
        let Some(entry) = state.agents.get_mut(&agent_key(&sid, &item.agent_id)) else { continue };
        if item.cwd.is_some() {
            entry.cwd.clone_from(&item.cwd);
        }
        if item.provider.is_some() {
            entry.provider.clone_from(&item.provider);
        }
        if entry.title.is_none() {
            entry.title = item.title.as_deref().map(|t| cap_string(t, SUMMARY_TITLE));
        }
        if entry.model.is_none() {
            entry.model = item.model.as_deref().map(|m| cap_string(m, LABEL));
        }
        if entry.last_activity.is_none() {
            entry.last_activity = item.last_activity;
        }
    }
}

fn drop_dialogs(state: &mut MonitorState, sid: &str, agent_id: &str) {
    state.ui_requests.retain(|_, u| !(u.server_id == sid && u.agent_id == agent_id));
}

fn close_if_target(state: &mut MonitorState, sid: &str, agent_id: &str, reason: &str, effects: &mut Vec<Effect>) {
    if state.drilldown.as_ref().is_some_and(|d| d.target.server_id == sid && d.target.agent_id == agent_id) {
        state.drilldown = None;
        effects.push(Effect::CloseDrill { reason: reason.to_owned() });
    }
}

/// `AgentArchived`: the card is hidden, its dialogs dropped. An unknown agent is ignored.
pub(crate) fn archived(state: &mut MonitorState, ctx: Ctx, agent_id: &str, effects: &mut Vec<Effect>) {
    let Some(sid) = owner(state, ctx) else { return };
    let Some(entry) = state.agents.get_mut(&agent_key(&sid, agent_id)) else { return };
    entry.archived = true;
    entry.attention.pending_dialogs = 0;
    drop_dialogs(state, &sid, agent_id);
    push_event(state, ctx, EventKind::AgentRemoved, agent_id, "agent archived");
    close_if_target(state, &sid, agent_id, "archived", effects);
}

/// `AgentDeleted`: the entry and its dialogs are removed.
pub(crate) fn deleted(state: &mut MonitorState, ctx: Ctx, agent_id: &str, effects: &mut Vec<Effect>) {
    let Some(sid) = owner(state, ctx) else { return };
    if state.agents.remove(&agent_key(&sid, agent_id)).is_some() {
        push_event(state, ctx, EventKind::AgentRemoved, agent_id, "agent deleted");
    }
    drop_dialogs(state, &sid, agent_id);
    close_if_target(state, &sid, agent_id, "deleted", effects);
}

/// `AgentStream`: update the card summary, feed the drill buffer when this agent is drilled
/// into. An event for an unknown agent is ignored and counted.
pub(crate) fn stream(state: &mut MonitorState, ctx: Ctx, agent_id: &str, seq: Option<u64>, event: &StreamEvent, effects: &mut Vec<Effect>) {
    let Some(sid) = owner(state, ctx) else { return };
    let Some(entry) = state.agents.get_mut(&agent_key(&sid, agent_id)) else {
        state.dropped_events = state.dropped_events.saturating_add(1);
        return;
    };
    let outcome = update_activity(entry, event);
    if outcome.failed_turn {
        push_event(state, ctx, EventKind::TurnFailed, agent_id, "turn failed");
    }
    if outcome.error_event {
        push_event(state, ctx, EventKind::AgentError, agent_id, "agent error");
    }
    if outcome.failed_turn || outcome.error_event {
        effects.push(fail_notification(&sid, agent_id));
    }
    if outcome.refresh_stats {
        effects.push(Effect::FetchStats { server_id: sid.clone(), agent_id: agent_id.to_owned() });
    }
    drill::apply_stream(state, &sid, agent_id, seq, event);
}
