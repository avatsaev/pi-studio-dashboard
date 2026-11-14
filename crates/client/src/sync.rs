//! Sync-on-connect: after `Hello`, `list_agents_request` and `agent_ui_list_request` run
//! concurrently (10 s each). Pushes that arrive meanwhile wait in a bounded buffer and are
//! replayed after the `Snapshot`, so a dialog resolved while the list was in flight is not
//! resurrected by the older list (D25, D27, D36).

use std::collections::VecDeque;

use futures_util::{SinkExt, StreamExt};
use pistudiodashboard_model::{AgentListItem, MutationResult, PendingUi};
use pistudiodashboard_protocol::{AgentStream, ListAgentsResponse, ListedAgent, OutboundRequest, UiListResponse, UiResolved, WireUiRequest, encode};
use serde_json::Value;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::tungstenite::Message;

use crate::conn::{Ended, Task, Ws};
use crate::requests::{DEFAULT_TIMEOUT, Kind, SyncKind};
use crate::rtt::Pinger;
use crate::state::{ConnError, ConnErrorKind};

/// Pushes held while syncing.
pub(crate) const MAX_BUFFERED: usize = 10_000;

/// Server pushes the dashboard understands. While syncing they are buffered; afterwards they
/// are dispatched as they arrive.
pub(crate) const PUSH_TYPES: &[&str] = &[
    "agent_update",
    "agent_status",
    "agent_archived",
    "agent_deleted",
    "agent_stream",
    "agent_timeline_reset",
    "agent_ui_request",
    "agent_ui_resolved",
    "daemon_status_update",
    "daemon_log_lines",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncFailure {
    TimedOut,
    Rejected,
}

/// Whether a push may be shed under pressure: text deltas and partial tool output
/// (attention-class: UI, status, `turn_*`, tool start/complete, errors).
fn is_foldable(message: &Value) -> bool {
    message.get("type").and_then(Value::as_str) == Some("agent_stream") && AgentStream::from_message(message).is_some_and(|s| s.event.is_foldable())
}

/// Bounded FIFO of pushes received while syncing.
#[derive(Default)]
pub(crate) struct PushBuffer {
    items: VecDeque<(Value, bool)>,
}

impl PushBuffer {
    /// Queue `message`. At capacity the oldest foldable push is dropped first (returns `true`);
    /// attention-class pushes are never dropped, so the buffer may exceed the cap if it holds
    /// nothing else.
    pub(crate) fn push(&mut self, message: Value) -> bool {
        let foldable = is_foldable(&message);
        let mut dropped = false;
        if self.items.len() >= MAX_BUFFERED {
            if let Some(i) = self.items.iter().position(|(_, f)| *f) {
                self.items.remove(i);
                dropped = true;
            } else if foldable {
                return true;
            }
        }
        self.items.push_back((message, foldable));
        dropped
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }
}

/// What the sync produced.
pub(crate) struct Synced {
    pub(crate) agents: Vec<AgentListItem>,
    pub(crate) ui_pending: Vec<PendingUi>,
    /// `agent_ui_list_request` failed: the connection is still live, with this recorded.
    pub(crate) warning: Option<ConnError>,
}

/// `key=value` per label (the model keeps flat strings).
fn agent_item(a: &ListedAgent) -> AgentListItem {
    AgentListItem {
        agent_id: a.agent_id.clone(),
        status: a.status.as_wire().to_owned(),
        title: a.title.clone(),
        cwd: a.cwd.clone(),
        labels: a.labels.iter().map(|(k, v)| format!("{k}={v}")).collect(),
        last_activity: a.last_activity,
        provider: a.provider.clone(),
        model: a.model.clone(),
        model_provider: a.model_provider.clone(),
        thinking_level: a.thinking_level.clone(),
    }
}

fn pending_item(r: &WireUiRequest) -> PendingUi {
    PendingUi {
        request_id: r.request_id.clone(),
        agent_id: r.agent_id.clone(),
        method: r.method.clone(),
        expects_response: r.expects_response,
        removed: r.removed,
        title: r.title().map(str::to_owned),
        message: r.message().map(str::to_owned),
        options: r.options(),
        timeout_ms: r.timeout_ms,
        created_at: r.created_at,
    }
}

enum SyncEvent {
    Frame(Option<Result<Message, WsError>>),
    Deadline,
}

async fn next_sync_event(ws: &mut Ws, deadline: Option<Instant>) -> SyncEvent {
    let due = async {
        match deadline {
            Some(d) => tokio::time::sleep_until(d).await,
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        biased;
        frame = ws.next() => SyncEvent::Frame(frame),
        () = due => SyncEvent::Deadline,
    }
}

fn failed(kind: ConnErrorKind, what: &str) -> ConnError {
    ConnError::new(kind, what)
}

impl Task {
    /// Run the sync. `Ok(Err(_))`: the sync failed and the connection must back off.
    pub(crate) async fn sync(&mut self, ws: &mut Ws) -> Result<Result<Synced, ConnError>, Ended> {
        self.rpc.reset();
        self.buffer = Some(PushBuffer::default());
        self.sync_done.clear();
        for (request, which) in [(OutboundRequest::ListAgents, SyncKind::Agents), (OutboundRequest::ListUi { agent_id: None }, SyncKind::Ui)] {
            let id = self.rpc.start(Kind::Sync(which), DEFAULT_TIMEOUT);
            if let Err(e) = ws.send(Message::text(encode(&request, &id))).await {
                return Ok(Err(failed(ConnErrorKind::Closed, &e.to_string())));
            }
        }
        let mut agents: Option<Result<Value, SyncFailure>> = None;
        let mut ui: Option<Result<Value, SyncFailure>> = None;
        loop {
            for (which, outcome) in std::mem::take(&mut self.sync_done) {
                match which {
                    SyncKind::Agents => agents = Some(outcome),
                    SyncKind::Ui => ui = Some(outcome),
                }
            }
            if let Some(Err(failure)) = &agents {
                return Ok(Err(match failure {
                    SyncFailure::TimedOut => failed(ConnErrorKind::Timeout, "list_agents_request timed out"),
                    SyncFailure::Rejected => failed(ConnErrorKind::BadHandshake, "list_agents_request was rejected"),
                }));
            }
            if let (Some(Ok(a)), Some(u)) = (&agents, &ui) {
                return Ok(self.finish_sync(a, u));
            }
            let deadline = self.rpc.next_deadline();
            match self.run_or_interrupt(next_sync_event(ws, deadline)).await? {
                SyncEvent::Deadline => {
                    for kind in self.rpc.expire(Instant::now()) {
                        self.finish(kind, None, MutationResult::Timeout);
                    }
                }
                SyncEvent::Frame(None | Some(Ok(Message::Close(_)))) => return Ok(Err(failed(ConnErrorKind::Closed, "closed during sync"))),
                SyncEvent::Frame(Some(Ok(Message::Text(text)))) => {
                    self.counters.message(Instant::now());
                    if let Some(too_large) = self.on_text(text.as_str(), &mut Pinger::default()) {
                        return Ok(Err(too_large));
                    }
                }
                SyncEvent::Frame(Some(Ok(Message::Binary(_)))) => {
                    self.counters.message(Instant::now());
                    self.counters.binary_ignored();
                }
                SyncEvent::Frame(Some(Ok(_))) => {}
                SyncEvent::Frame(Some(Err(WsError::Capacity(_)))) => {
                    self.counters.frame_too_large();
                    return Ok(Err(failed(ConnErrorKind::FrameTooLarge, "frame over the size limit")));
                }
                SyncEvent::Frame(Some(Err(e))) => return Ok(Err(failed(ConnErrorKind::Closed, &e.to_string()))),
            }
        }
    }

    fn finish_sync(&mut self, agents: &Value, ui: &Result<Value, SyncFailure>) -> Result<Synced, ConnError> {
        let Some(list) = ListAgentsResponse::from_message(agents) else {
            return Err(failed(ConnErrorKind::BadHandshake, "list_agents_response is malformed"));
        };
        for _ in 0..list.malformed {
            self.counters.malformed();
        }
        for _ in 0..list.unknown_status {
            self.counters.unknown_enum_value();
        }
        let mut warning = None;
        let mut ui_pending = Vec::new();
        match ui.as_ref().ok().and_then(UiListResponse::from_message) {
            Some(r) if r.ok => {
                for _ in 0..r.malformed {
                    self.counters.malformed();
                }
                ui_pending = r.pending.iter().filter(|p| p.is_attention()).map(pending_item).collect();
            }
            _ => {
                let why = match ui {
                    Err(SyncFailure::TimedOut) => "timed out",
                    Err(SyncFailure::Rejected) => "was rejected",
                    Ok(_) => "failed",
                };
                log::warn!("daemon entry {}: agent_ui_list_request {why}; continuing without pending dialogs", self.em.entry_id());
                warning = Some(failed(ConnErrorKind::Timeout, &format!("agent_ui_list_request {why}")));
            }
        }
        Ok(Synced { agents: list.agents.iter().map(agent_item).collect(), ui_pending, warning })
    }

    /// A known push arrived: hold it while syncing, else dispatch.
    pub(crate) fn on_push(&mut self, message: Value) {
        match &mut self.buffer {
            Some(buffer) => {
                if buffer.push(message) {
                    self.counters.dropped_event();
                }
            }
            None => self.dispatch_push(&message),
        }
    }

    /// Turn a push into events. Typed events for the other pushes arrive with sprints 006/007.
    pub(crate) fn dispatch_push(&mut self, message: &Value) {
        let ty = message.get("type").and_then(Value::as_str).unwrap_or("");
        self.agent_push(ty, message);
        if ty == "agent_ui_resolved" {
            match UiResolved::from_message(message) {
                Some(r) => {
                    self.out.push(pistudiodashboard_model::DaemonEventKind::UiResolved { request_id: r.request_id, agent_id: r.agent_id, reason: r.reason })
                }
                None => self.counters.malformed(),
            }
        }
    }

    /// After the `Snapshot`: replay what was held, in arrival order.
    pub(crate) async fn replay_buffered(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            for (message, _) in buffer.items {
                self.dispatch_push(&message);
            }
        }
        self.flush_out().await;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A foldable delta (`"agent_stream"`) or an attention-class push (anything else).
    fn push(ty: &str, n: usize) -> Value {
        if ty == "agent_stream" {
            json!({"type": ty, "agentId": "a", "n": n, "event": {"kind": "assistant_message", "text": "x"}})
        } else if ty == "turn" {
            json!({"type": "agent_stream", "agentId": "a", "n": n, "seq": n, "event": {"kind": "turn_failed"}})
        } else {
            json!({"type": ty, "n": n})
        }
    }

    #[test]
    fn only_deltas_and_partials_are_foldable() {
        assert!(is_foldable(&push("agent_stream", 1)));
        assert!(!is_foldable(&push("turn", 1)));
        assert!(!is_foldable(&push("agent_ui_request", 1)));
        assert!(!is_foldable(&json!({"type": "agent_stream"})));
    }

    #[test]
    fn buffer_sheds_foldable_pushes_first_and_never_attention_ones() {
        let mut b = PushBuffer::default();
        for i in 0..MAX_BUFFERED {
            assert!(!b.push(push(if i % 2 == 0 { "agent_stream" } else { "agent_ui_request" }, i)));
        }
        // Full: a new attention push evicts the oldest foldable one.
        assert!(b.push(push("agent_ui_request", 1)));
        assert_eq!(b.len(), MAX_BUFFERED);
        assert_eq!(b.items.front().unwrap().0["n"], 1, "oldest foldable (n=0) was dropped");
        // A new foldable push is accepted by dropping another foldable.
        assert!(b.push(push("agent_stream", 2)));
        assert_eq!(b.len(), MAX_BUFFERED);
        let attention = b.items.iter().filter(|(_, f)| !*f).count();
        assert_eq!(attention, MAX_BUFFERED / 2 + 1);
    }

    #[test]
    fn buffer_of_only_attention_pushes_grows_rather_than_dropping() {
        let mut b = PushBuffer::default();
        for i in 0..MAX_BUFFERED + 5 {
            assert!(!b.push(push("agent_ui_request", i)));
        }
        assert_eq!(b.len(), MAX_BUFFERED + 5);
        // A foldable push at capacity with nothing foldable to evict is itself dropped.
        assert!(b.push(push("agent_stream", 0)));
        assert_eq!(b.len(), MAX_BUFFERED + 5);
    }
}
