#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pistudiodashboard_ingest::apply_in_place;
use pistudiodashboard_model::{
    AgentListItem, ConnState, DaemonEvent, DaemonEventKind, DaemonSlice, Effect, MonitorState, PendingUi, ServerCaps, ServerInfoView,
};

pub fn state(entries: &[&str]) -> MonitorState {
    let mut s = MonitorState::default();
    for e in entries {
        s.daemons.push(DaemonSlice::new(*e, *e));
    }
    s
}

pub fn ev(entry: &str, epoch: u64, kind: DaemonEventKind) -> DaemonEvent {
    DaemonEvent { entry_id: entry.into(), server_id: None, conn_epoch: epoch, seq: 0, at: 1000, kind }
}

pub fn hello(entry: &str, epoch: u64, sid: &str) -> DaemonEvent {
    let mut e = ev(
        entry,
        epoch,
        DaemonEventKind::Hello {
            info: ServerInfoView::default(),
            caps: ServerCaps::default(),
            degraded: Default::default(),
            tls_in_use: false,
            is_loopback: true,
        },
    );
    e.server_id = Some(sid.into());
    e
}

pub fn conn(entry: &str, epoch: u64, s: ConnState) -> DaemonEvent {
    ev(entry, epoch, DaemonEventKind::ConnState { state: s, since: 1, last_error: None, next_retry_in_ms: None })
}

pub fn item(id: &str, status: &str) -> AgentListItem {
    AgentListItem { agent_id: id.into(), status: status.into(), title: Some(format!("t-{id}")), ..AgentListItem::default() }
}

pub fn pending(req: &str, agent: &str) -> PendingUi {
    PendingUi { request_id: req.into(), agent_id: agent.into(), method: "confirm".into(), expects_response: true, created_at: 5, ..PendingUi::default() }
}

pub fn snapshot(entry: &str, epoch: u64, agents: Vec<AgentListItem>, ui: Vec<PendingUi>) -> DaemonEvent {
    ev(entry, epoch, DaemonEventKind::Snapshot { agents, ui_pending: ui })
}

/// Apply events in place and return all effects.
pub fn run(s: &mut MonitorState, events: &[DaemonEvent], now: i64) -> Vec<Effect> {
    let mut fx = Vec::new();
    for e in events {
        apply_in_place(s, e, now, &mut fx);
    }
    fx
}

/// A live daemon `e1` (server `srv`) with these agents `(id, status)` already snapshotted.
pub fn live_state(agents: &[(&str, &str)]) -> MonitorState {
    let mut s = state(&["e1"]);
    let items = agents.iter().map(|(id, st)| item(id, st)).collect();
    run(&mut s, &[hello("e1", 1, "srv"), conn("e1", 1, ConnState::Live), snapshot("e1", 1, items, vec![])], 100);
    s
}

pub fn kind(k: DaemonEventKind) -> DaemonEvent {
    ev("e1", 1, k)
}

pub fn stream(agent: &str, seq: Option<u64>, event: pistudiodashboard_model::StreamEvent) -> DaemonEvent {
    kind(DaemonEventKind::AgentStream { agent_id: agent.into(), seq, timestamp: None, event })
}

pub fn update(agent: &str, status: Option<&str>) -> DaemonEvent {
    kind(DaemonEventKind::AgentUpdate { agent_id: agent.into(), status: status.map(Into::into), title: None, labels: None, model: None, model_provider: None })
}
