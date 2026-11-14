//! Top-level reducer: epoch gate, then dispatch per event kind.

use pistudiodashboard_model::{
    ConnErrorView, ConnState, CountersSnapshot, DaemonEvent, DaemonEventKind, DaemonSlice, Effect, EventKind, EventRecord, Health, LABEL, MonitorState,
    UNKNOWN_TYPES_SHOWN, UnknownTypeCount, cap_string,
};

use crate::identity::resolve_identity;
use crate::rebuild::{mark_stale, rebuild};
use crate::{drill, reduce_agent, reduce_stats};

#[derive(Clone, Copy)]
pub(crate) struct Ctx {
    pub idx: usize,
    pub at: i64,
    pub now: i64,
}

/// Apply one event to a copy of `state`.
#[must_use]
pub fn apply(state: &MonitorState, event: &DaemonEvent, now: i64) -> (MonitorState, Vec<Effect>) {
    apply_batch(state, std::slice::from_ref(event), now)
}

/// Apply `events` in order on a single working copy: one published state per batch.
#[must_use]
pub fn apply_batch(state: &MonitorState, events: &[DaemonEvent], now: i64) -> (MonitorState, Vec<Effect>) {
    let mut next = state.clone();
    let mut effects = Vec::new();
    for event in events {
        apply_in_place(&mut next, event, now, &mut effects);
    }
    (next, effects)
}

/// Apply one event in place, appending requested effects. Total: never panics.
///
/// An event for an unknown `entry_id` is dropped and counted in `state.dropped_events`. An event
/// from an older connection epoch than the slice's is dropped silently (D38).
pub fn apply_in_place(state: &mut MonitorState, event: &DaemonEvent, now: i64, effects: &mut Vec<Effect>) {
    let Some(idx) = state.daemons.iter().position(|d| d.entry_id == event.entry_id) else {
        state.dropped_events = state.dropped_events.saturating_add(1);
        return;
    };
    let Some(slice) = state.daemons.get_mut(idx) else { return };
    if event.conn_epoch < slice.conn_epoch {
        return;
    }
    let is_conn_state = matches!(event.kind, DaemonEventKind::ConnState { .. });
    if is_conn_state && event.conn_epoch > slice.conn_epoch {
        slice.conn_epoch = event.conn_epoch;
    }
    let ctx = Ctx { idx, at: event.at, now };
    match &event.kind {
        DaemonEventKind::ConnState { state: s, since, last_error, next_retry_in_ms } => {
            conn_state(state, ctx, *s, *since, last_error.as_ref(), *next_retry_in_ms);
        }
        DaemonEventKind::Hello { info, caps, degraded, tls_in_use, is_loopback } => {
            if let Some(slice) = state.daemons.get_mut(idx) {
                slice.info = Some(info.clone());
                slice.caps = caps.clone();
                slice.degraded = degraded.clone();
                slice.tls_in_use = *tls_in_use;
                slice.is_loopback = *is_loopback;
            }
            if let Some(sid) = event.server_id.as_deref() {
                resolve_identity(state, idx, sid, effects);
            }
        }
        DaemonEventKind::Health { reachable, at } => {
            if let Some(slice) = state.daemons.get_mut(idx) {
                slice.health = Some(Health { reachable: *reachable, at: *at });
            }
        }
        DaemonEventKind::Snapshot { agents, ui_pending } => rebuild(state, ctx, agents, ui_pending, effects),
        DaemonEventKind::Rtt { ms } => {
            if let Some(slice) = state.daemons.get_mut(idx) {
                slice.counters.rtt_last_ms = Some(*ms);
            }
        }
        DaemonEventKind::Diagnostics(c) => {
            if let Some(slice) = state.daemons.get_mut(idx) {
                store_diagnostics(slice, c);
            }
        }
        // Mutation outcomes are reduced by sprint-007/task-002 (inFlight, toasts).
        DaemonEventKind::RequestResult { .. } => {}
        // Dialog resolution is reduced by sprint-007/task-001.
        DaemonEventKind::UiResolved { .. } => {}
        DaemonEventKind::AgentUpdate { agent_id, status, title, labels, model, model_provider: _ } => {
            let patch = reduce_agent::Patch {
                status: status.as_deref(),
                title: title.as_ref().map(|t| t.as_deref()),
                labels: labels.as_deref(),
                model: model.as_deref(),
            };
            reduce_agent::update(state, ctx, agent_id, &patch, effects);
        }
        DaemonEventKind::AgentStatus { agent_id, status } => {
            let patch = reduce_agent::Patch { status: Some(status), title: None, labels: None, model: None };
            reduce_agent::update(state, ctx, agent_id, &patch, effects);
        }
        DaemonEventKind::AgentArchived { agent_id, .. } => reduce_agent::archived(state, ctx, agent_id, effects),
        DaemonEventKind::AgentDeleted { agent_id } => reduce_agent::deleted(state, ctx, agent_id, effects),
        DaemonEventKind::AgentStream { agent_id, seq, event: stream_event, .. } => reduce_agent::stream(state, ctx, agent_id, *seq, stream_event, effects),
        DaemonEventKind::TimelineReset { agent_id, .. } => drill::timeline_reset(state, ctx, agent_id, effects),
        DaemonEventKind::Stats { agent_id, result } => reduce_stats::stats(state, ctx, agent_id, result),
        DaemonEventKind::Timeline { agent_id, items, reset, seq_end } => drill::timeline(state, ctx, agent_id, items, *reset, *seq_end),
    }
}

fn conn_state(state: &mut MonitorState, ctx: Ctx, new: ConnState, since: i64, last_error: Option<&ConnErrorView>, next_retry_in_ms: Option<u64>) {
    if let Some(slice) = state.daemons.get_mut(ctx.idx) {
        slice.conn = new;
        slice.conn_since = Some(since);
        slice.last_error = last_error.cloned();
        slice.next_retry_in_ms = next_retry_in_ms;
        slice.events.push(EventRecord { at: ctx.at, kind: EventKind::ConnState, agent_id: None, summary: format!("connection {}", new.as_str()) });
    }
    if matches!(new, ConnState::Backoff | ConnState::AuthFailed | ConnState::HostRejected) {
        mark_stale(state, ctx.idx, ctx.now);
    }
}

fn store_diagnostics(slice: &mut DaemonSlice, c: &CountersSnapshot) {
    let k = &mut slice.counters;
    k.unknown = c.unknown_types.values().fold(0u64, |a, n| a.saturating_add(*n));
    let mut top: Vec<UnknownTypeCount> =
        c.unknown_types.iter().map(|(name, count)| UnknownTypeCount { name: cap_string(name, LABEL), count: *count }).collect();
    top.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    top.truncate(UNKNOWN_TYPES_SHOWN);
    k.unknown_types = top;
    k.malformed = c.malformed;
    k.unknown_enum_values = c.unknown_enum_values;
    k.reconnect_count = c.reconnect_count;
    k.messages_total = c.messages_total;
    k.messages_per_second = c.messages_per_second;
    k.binary_frames_ignored = c.binary_frames_ignored;
    k.frames_too_large = c.frames_too_large;
    k.dropped_events = c.dropped_events;
    k.late_responses = c.late_responses;
    k.rtt_last_ms = c.rtt_last_ms;
    k.rtt_ewma_ms = c.rtt_ewma_ms;
    k.connected_since = c.connected_since;
}
