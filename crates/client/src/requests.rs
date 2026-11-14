//! Outbound request path and correlation (D2, D27).
//!
//! `ClientHandle::request` enqueues an [`OutboundRequest`] on the daemon's bounded command
//! channel with a non-blocking `try_send`; only a `live` connection has such a channel, so a
//! request while not live fails at once and is never queued or replayed (mutations: D27). The
//! connection task owns the socket and the pending table; this module is the shared glue.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use pistudiodashboard_model::{MutationOp, MutationResult};
use pistudiodashboard_protocol::reply::{INTERRUPT_REPLY_TYPE, RESPOND_REPLY_TYPE};
use pistudiodashboard_protocol::{InterruptReply, OutboundRequest, RequestClass, RespondUiReply, RpcError};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::Instant;

/// Pending requests per connection; beyond it non-mutation requests fail fast with `Busy`.
pub(crate) const MAX_PENDING: usize = 256;
/// Commands waiting for the connection task.
const COMMAND_QUEUE: usize = 512;
/// Remembered timed-out request ids, to recognise (and count) late answers.
const MAX_TIMED_OUT: usize = 256;

pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const LONG_TIMEOUT: Duration = Duration::from_secs(15);

/// Why a request was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// The daemon is unknown or not `live`.
    NotConnected,
    /// 256 requests are already outstanding (reads only).
    Busy,
    /// The command queue of the connection task is full.
    QueueFull,
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotConnected => "daemon is not connected",
            Self::Busy => "too many outstanding requests",
            Self::QueueFull => "request queue is full",
        })
    }
}

impl std::error::Error for SendError {}

pub(crate) struct Command {
    pub(crate) request: OutboundRequest,
    pub(crate) token: String,
}

/// State shared between `ClientHandle::request` and the connection task of one entry.
#[derive(Default)]
pub(crate) struct Shared {
    server_id: Mutex<Option<String>>,
    cmd: Mutex<Option<mpsc::Sender<Command>>>,
    /// Accepted-but-unfinished requests (queued + pending).
    inflight: AtomicUsize,
    /// Agents with a `SessionStats` request queued or in flight (one per agent, D16).
    stats_agents: Mutex<std::collections::HashSet<String>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    pub(crate) fn set_server_id(&self, id: &str) {
        *lock(&self.server_id) = Some(id.to_owned());
    }

    pub(crate) fn server_id(&self) -> Option<String> {
        lock(&self.server_id).clone()
    }

    /// The connection became `live`: start accepting commands.
    pub(crate) fn publish(&self) -> mpsc::Receiver<Command> {
        let (tx, rx) = mpsc::channel(COMMAND_QUEUE);
        self.inflight.store(0, Ordering::SeqCst);
        *lock(&self.cmd) = Some(tx);
        rx
    }

    /// The connection left `live`: stop accepting commands.
    pub(crate) fn withdraw(&self) {
        *lock(&self.cmd) = None;
        lock(&self.stats_agents).clear();
    }

    /// A stats request ended: the agent may be asked again.
    pub(crate) fn stats_done(&self, agent_id: &str) {
        lock(&self.stats_agents).remove(agent_id);
    }

    /// A pending (or queued) request finished.
    pub(crate) fn finished(&self) {
        let _ = self.inflight.try_update(Ordering::SeqCst, Ordering::SeqCst, |n| Some(n.saturating_sub(1)));
    }

    pub(crate) fn try_enqueue(&self, request: OutboundRequest, token: String) -> Result<(), SendError> {
        if let OutboundRequest::SessionStats { agent_id } = &request {
            return self.enqueue_stats(agent_id.clone(), request, token);
        }
        let read = request.class() == RequestClass::Read;
        let before = self.inflight.fetch_add(1, Ordering::SeqCst);
        if read && before >= MAX_PENDING {
            self.finished();
            return Err(SendError::Busy);
        }
        let sent = match lock(&self.cmd).as_ref() {
            None => Err(SendError::NotConnected),
            Some(tx) => tx.try_send(Command { request, token }).map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => SendError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => SendError::NotConnected,
            }),
        };
        if sent.is_err() {
            self.finished();
        }
        sent
    }
}

impl Shared {
    /// Stats are not counted against the read limit: one per agent, at most 4 sent at a time.
    /// A request for an agent that already has one queued or in flight is dropped (`Ok`).
    fn enqueue_stats(&self, agent_id: String, request: OutboundRequest, token: String) -> Result<(), SendError> {
        let guard = lock(&self.cmd);
        let Some(tx) = guard.as_ref() else { return Err(SendError::NotConnected) };
        if !lock(&self.stats_agents).insert(agent_id.clone()) {
            return Ok(());
        }
        tx.try_send(Command { request, token }).map_err(|e| {
            lock(&self.stats_agents).remove(&agent_id);
            match e {
                mpsc::error::TrySendError::Full(_) => SendError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => SendError::NotConnected,
            }
        })
    }
}

/// What a pending request is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A read whose answer is not consumed yet (stats, timeline: later tasks).
    Read,
    Mutation {
        op: MutationOp,
        token: String,
    },
    /// A sync-on-connect request, consumed by the connection task itself.
    Sync(SyncKind),
    /// A `ListAgents` sent by the engine after the connect-time sync (the answer becomes an `AgentsListed` event).
    AgentList,
    /// A `SessionStats` request (executor side; the answer becomes a `Stats` event).
    Stats {
        agent_id: String,
    },
    /// One page of a timeline load; `first` is the engine's request, later pages are the pager's.
    Timeline {
        job: u64,
        first: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncKind {
    Agents,
    Ui,
}

impl Kind {
    pub(crate) fn of(request: &OutboundRequest, token: String) -> Self {
        match request {
            OutboundRequest::RespondUi { .. } => Self::Mutation { op: MutationOp::RespondUi, token },
            OutboundRequest::InterruptAgent { .. } => Self::Mutation { op: MutationOp::InterruptAgent, token },
            OutboundRequest::ListAgents => Self::AgentList,
            _ => Self::Read,
        }
    }
}

pub(crate) fn timeout_for(request: &OutboundRequest) -> Duration {
    match request {
        OutboundRequest::SessionStats { .. }
        | OutboundRequest::FetchTimeline { .. }
        | OutboundRequest::FetchTimelinePage { .. }
        | OutboundRequest::RespondUi { .. }
        | OutboundRequest::InterruptAgent { .. } => LONG_TIMEOUT,
        OutboundRequest::Ping { .. } | OutboundRequest::ListAgents | OutboundRequest::ListUi { .. } => DEFAULT_TIMEOUT,
    }
}

struct Pending {
    kind: Kind,
    deadline: Instant,
}

/// The pending table of one connection. Ids are `<entry-short>-<counter>`.
pub(crate) struct Rpc {
    prefix: String,
    next: u64,
    pending: HashMap<String, Pending>,
    timed_out: VecDeque<String>,
}

impl Rpc {
    pub(crate) fn new(entry_id: &str) -> Self {
        Self { prefix: entry_id.chars().take(8).collect(), next: 0, pending: HashMap::new(), timed_out: VecDeque::new() }
    }

    /// A new connection: counter and tables restart.
    pub(crate) fn reset(&mut self) {
        self.next = 0;
        self.pending.clear();
        self.timed_out.clear();
    }

    /// Register a pending request; returns its id.
    pub(crate) fn start(&mut self, kind: Kind, timeout: Duration) -> String {
        self.next += 1;
        let id = format!("{}-{}", self.prefix, self.next);
        self.pending.insert(id.clone(), Pending { kind, deadline: Instant::now() + timeout });
        id
    }

    pub(crate) fn complete(&mut self, request_id: &str) -> Option<Kind> {
        self.pending.remove(request_id).map(|p| p.kind)
    }

    /// `true` once for an id that timed out earlier (a late answer).
    pub(crate) fn take_late(&mut self, request_id: &str) -> bool {
        match self.timed_out.iter().position(|id| id == request_id) {
            Some(i) => {
                self.timed_out.remove(i);
                true
            }
            None => false,
        }
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().map(|p| p.deadline).min()
    }

    /// Remove everything past its deadline; the ids are remembered for late answers.
    pub(crate) fn expire(&mut self, now: Instant) -> Vec<Kind> {
        let due: Vec<String> = self.pending.iter().filter(|(_, p)| p.deadline <= now).map(|(id, _)| id.clone()).collect();
        let mut kinds = Vec::with_capacity(due.len());
        for id in due {
            if let Some(p) = self.pending.remove(&id) {
                kinds.push(p.kind);
                if self.timed_out.len() >= MAX_TIMED_OUT {
                    self.timed_out.pop_front();
                }
                self.timed_out.push_back(id);
            }
        }
        kinds
    }

    /// The connection is gone: every pending request ends here.
    pub(crate) fn drain(&mut self) -> Vec<Kind> {
        self.timed_out.clear();
        self.pending.drain().map(|(_, p)| p.kind).collect()
    }
}

/// Map a daemon reply to a mutation outcome.
pub(crate) fn mutation_outcome(op: MutationOp, message: &Value) -> MutationResult {
    let ty = message.get("type").and_then(Value::as_str);
    if ty == Some("rpc_error") {
        return match RpcError::from_message(message) {
            Some(e) => MutationResult::RpcError { code: e.code, message: e.message },
            None => MutationResult::RpcError { code: None, message: None },
        };
    }
    match (op, ty) {
        (MutationOp::RespondUi, Some(RESPOND_REPLY_TYPE)) => match RespondUiReply::from_message(message) {
            Some(r) if r.ok => MutationResult::Ok,
            Some(r) if r.error.as_deref() == Some("not_found") => MutationResult::NotFound,
            Some(r) => MutationResult::Rejected { error: r.error.unwrap_or_else(|| "unknown".to_owned()) },
            None => MutationResult::Rejected { error: "malformed_reply".to_owned() },
        },
        (MutationOp::InterruptAgent, Some(INTERRUPT_REPLY_TYPE)) => match InterruptReply::from_message(message) {
            Some(r) => MutationResult::Interrupt { ok: r.ok },
            None => MutationResult::Rejected { error: "malformed_reply".to_owned() },
        },
        _ => MutationResult::Rejected { error: "unexpected_reply".to_owned() },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn mutation_outcomes() {
        let respond = |p: Value| json!({"type":"agent_ui_respond_response","requestId":"r","payload":p});
        assert_eq!(mutation_outcome(MutationOp::RespondUi, &respond(json!({"ok":true}))), MutationResult::Ok);
        assert_eq!(mutation_outcome(MutationOp::RespondUi, &respond(json!({"ok":false,"error":"not_found"}))), MutationResult::NotFound);
        assert_eq!(mutation_outcome(MutationOp::RespondUi, &respond(json!({"ok":false,"error":"boom"}))), MutationResult::Rejected { error: "boom".into() });
        let i = |ok: bool| json!({"type":"interrupt_response","requestId":"r","ok":ok});
        assert_eq!(mutation_outcome(MutationOp::InterruptAgent, &i(false)), MutationResult::Interrupt { ok: false });
        assert_eq!(mutation_outcome(MutationOp::InterruptAgent, &i(true)), MutationResult::Interrupt { ok: true });
        let err = json!({"type":"rpc_error","requestId":"r","code":"handler_error","message":"x"});
        assert_eq!(
            mutation_outcome(MutationOp::InterruptAgent, &err),
            MutationResult::RpcError { code: Some("handler_error".into()), message: Some("x".into()) }
        );
        assert!(matches!(mutation_outcome(MutationOp::InterruptAgent, &respond(json!({"ok":true}))), MutationResult::Rejected { .. }));
    }

    #[tokio::test(start_paused = true)]
    async fn ids_timeouts_and_late_answers() {
        let mut rpc = Rpc::new("abcdefghijkl");
        let a = rpc.start(Kind::Read, DEFAULT_TIMEOUT);
        let b = rpc.start(Kind::Read, LONG_TIMEOUT);
        assert_eq!((a.as_str(), b.as_str()), ("abcdefgh-1", "abcdefgh-2"));
        tokio::time::advance(Duration::from_secs(11)).await;
        assert_eq!(rpc.expire(Instant::now()), [Kind::Read]);
        assert!(rpc.take_late(&a) && !rpc.take_late(&a));
        assert_eq!(rpc.complete(&b), Some(Kind::Read));
        let _c = rpc.start(Kind::Read, DEFAULT_TIMEOUT);
        assert_eq!(rpc.drain(), [Kind::Read]);
        rpc.reset();
        assert_eq!(rpc.start(Kind::Read, DEFAULT_TIMEOUT), "abcdefgh-1");
    }
}
