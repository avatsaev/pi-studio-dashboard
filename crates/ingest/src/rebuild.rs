//! Rebuild-on-connect (atomic per-daemon replace) and stale marking.
//!
//! `ui_pending` conversion is deliberately minimal: items with `expects_response` and not
//! `removed` become `UiRequestItem`s with bounded text, `kind = Other` and `HandOff`. Method
//! classification (`from_wire`, answerability, expiry controls) is sprint-007/task-001.

use std::collections::BTreeMap;

use pistudiodashboard_model::{
    AgentEntry, AgentListItem, AgentStatus, Answerability, Attention, DIALOG_MESSAGE, DIALOG_OPTION, DIALOG_OPTIONS, DIALOG_TITLE, DrillState, Effect, LABEL,
    MonitorState, PendingUi, RequestKind, SUMMARY_TITLE, UiRequestItem, UsageSnapshot, agent_key, cap_string, ui_request_key,
};

use crate::apply::Ctx;
use crate::keys::{server_keys, take_server};

const AGENT_GONE: &str = "agent gone";

/// `stale_since ??= now`; flag the daemon's agents, dialogs and drill buffer. Nothing is dropped.
/// A duplicate slice owns nothing, so only its timestamp is set.
pub(crate) fn mark_stale(state: &mut MonitorState, idx: usize, now: i64) {
    let Some(slice) = state.daemons.get_mut(idx) else { return };
    let Some(sid) = slice.server_id.clone() else { return };
    slice.stale_since.get_or_insert(now);
    if slice.duplicate_of.is_some() {
        return;
    }
    for key in server_keys(&state.agents, &sid) {
        if let Some(a) = state.agents.get_mut(&key) {
            a.stale = true;
        }
    }
    for key in server_keys(&state.ui_requests, &sid) {
        if let Some(u) = state.ui_requests.get_mut(&key) {
            u.stale = true;
            u.in_flight = false;
            u.in_flight_response = None;
        }
    }
    if let Some(d) = state.drilldown.as_mut().filter(|d| d.target.server_id == sid) {
        d.stale = true;
    }
}

fn agent_of(a: &AgentEntry, sid: &str) -> bool {
    a.server_id == sid
}

fn dialog_of(u: &UiRequestItem, sid: &str) -> bool {
    u.server_id == sid
}

fn to_agent(sid: &str, item: &AgentListItem, old: Option<&AgentEntry>) -> AgentEntry {
    let status = AgentStatus::from_wire(&item.status);
    let running = status == AgentStatus::Running;
    AgentEntry {
        server_id: sid.to_owned(),
        agent_id: item.agent_id.clone(),
        title: item.title.as_deref().map(|t| cap_string(t, SUMMARY_TITLE)),
        cwd: item.cwd.clone(),
        labels: item.labels.iter().map(|l| cap_string(l, LABEL)).collect(),
        provider: item.provider.clone(),
        model: item.model.clone(),
        status,
        last_activity: item.last_activity,
        archived: false,
        activity: old.filter(|_| running).map(|o| o.activity.clone()).unwrap_or_default(),
        usage: old.map(|o| o.usage.clone()).unwrap_or_default(),
        attention: Attention { pending_dialogs: 0, errored: status == AgentStatus::Error },
        stale: false,
        assistant_tail: old.filter(|_| running).map(|o| o.assistant_tail.clone()).unwrap_or_default(),
    }
}

fn to_dialog(sid: &str, p: &PendingUi, old: Option<&UiRequestItem>, now: i64) -> UiRequestItem {
    let options = p.options.as_ref().map(|o| o.iter().take(DIALOG_OPTIONS).map(|s| cap_string(s, DIALOG_OPTION)).collect());
    UiRequestItem {
        server_id: sid.to_owned(),
        request_id: p.request_id.clone(),
        agent_id: p.agent_id.clone(),
        method: p.method.clone(),
        kind: RequestKind::Other,
        answerability: Answerability::HandOff,
        title: p.title.as_deref().map(|t| cap_string(t, DIALOG_TITLE)),
        message: p.message.as_deref().map(|t| cap_string(t, DIALOG_MESSAGE)),
        options_truncated: p.options.as_ref().is_some_and(|o| o.len() > DIALOG_OPTIONS),
        options,
        timeout_ms: p.timeout_ms,
        created_at: p.created_at,
        expires_at: p.timeout_ms.and_then(|t| i64::try_from(t).ok()).and_then(|t| p.created_at.checked_add(t)),
        raised_at: old.map_or(now, |o| o.raised_at),
        in_flight: false,
        in_flight_response: None,
        stale: false,
    }
}

/// Atomic replace of one daemon's agents and dialogs from a snapshot (D8, D25).
///
/// A snapshot for a slice with no `server_id` yet (before `Hello`) or a duplicate slice cannot
/// be keyed or must not be double counted: it is dropped and counted in `dropped_events`.
pub(crate) fn rebuild(state: &mut MonitorState, ctx: Ctx, agents: &[AgentListItem], pending: &[PendingUi], effects: &mut Vec<Effect>) {
    let Some(slice) = state.daemons.get(ctx.idx) else { return };
    let sid = match (&slice.server_id, &slice.duplicate_of) {
        (Some(sid), None) => sid.clone(),
        _ => {
            state.dropped_events = state.dropped_events.saturating_add(1);
            return;
        }
    };
    let old_agents = take_server(&mut state.agents, &sid, agent_of);
    let old_ui = take_server(&mut state.ui_requests, &sid, dialog_of);

    let mut new_ui = BTreeMap::new();
    let mut per_agent: BTreeMap<&str, u32> = BTreeMap::new();
    for p in pending.iter().filter(|p| p.expects_response && !p.removed) {
        let key = ui_request_key(&sid, &p.request_id);
        let item = to_dialog(&sid, p, old_ui.get(&key), ctx.now);
        new_ui.insert(key, item);
    }
    for u in new_ui.values() {
        let n = per_agent.entry(u.agent_id.as_str()).or_insert(0);
        *n = n.saturating_add(1);
    }
    let mut new_agents = BTreeMap::new();
    for item in agents {
        let key = agent_key(&sid, &item.agent_id);
        let mut entry = to_agent(&sid, item, old_agents.get(&key));
        entry.attention.pending_dialogs = per_agent.get(item.agent_id.as_str()).copied().unwrap_or(0);
        new_agents.insert(key, entry);
    }
    for a in new_agents.values() {
        if a.status == AgentStatus::Running || a.usage == UsageSnapshot::None {
            effects.push(Effect::FetchStats { server_id: sid.clone(), agent_id: a.agent_id.clone() });
        }
    }
    state.agents.append(&mut new_agents);
    state.ui_requests.append(&mut new_ui);
    if let Some(slice) = state.daemons.get_mut(ctx.idx) {
        slice.stale_since = None;
    }
    refresh_drill(state, &sid, effects);
}

fn refresh_drill(state: &mut MonitorState, sid: &str, effects: &mut Vec<Effect>) {
    let Some(target) = state.drilldown.as_ref().map(|d| d.target.clone()).filter(|t| t.server_id == sid) else { return };
    let present = state.agents.get(&agent_key(sid, &target.agent_id)).is_some_and(|a| !a.archived);
    if present {
        if let Some(d) = state.drilldown.as_mut() {
            d.state = DrillState::Refetching;
            d.stale = false;
        }
        effects.push(Effect::FetchTimeline { server_id: sid.to_owned(), agent_id: target.agent_id });
    } else {
        state.drilldown = None;
        effects.push(Effect::CloseDrill { reason: AGENT_GONE.to_owned() });
    }
}
