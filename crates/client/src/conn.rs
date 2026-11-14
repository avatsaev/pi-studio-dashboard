//! One connection task per entry: the state machine
//! `idle -> probing -> connecting -> handshaking -> syncing -> live`, any -> `backoff -> probing`,
//! terminals `auth_failed` / `host_rejected` / user `idle`.

use std::collections::BTreeSet;
use std::future::Future;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use pistudiodashboard_config::DaemonEntry;
use pistudiodashboard_model::{ConnState, DaemonEventKind, DegradedReason, MutationResult, ServerCaps, ServerInfoView};
use pistudiodashboard_protocol::{FrameError, Hello, Inbound, MAX_FRAME_BYTES, Pong, ServerInfo, decode_inbound, encode};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{Instant, Interval, MissedTickBehavior};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

use crate::agents::{StatsExec, TimelineExec};
use crate::backoff::{self, CAP_ATTEMPT};
use crate::counters::Counters;
use crate::emitter::{Emitter, now_ms};
use crate::probe;
use crate::requests::{Command, Kind, Rpc, Shared, SyncKind, mutation_outcome, timeout_for};
use crate::rtt::{Pinger, Tick};
use crate::secret::Secret;
use crate::state::{ConnError, ConnErrorKind};
use crate::sync::{PUSH_TYPES, PushBuffer, SyncFailure};
use crate::tls;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_INFO_TIMEOUT: Duration = Duration::from_secs(5);
const BEARER_PREFIX: &str = "pi-studio.bearer.";
const DIAGNOSTICS_EVERY: Duration = Duration::from_secs(1);
/// Consecutive handshake failures after which the error is `handshake_rejected` and the delay stays at the cap.
const HANDSHAKE_REJECTED_AFTER: u32 = 3;

/// Session message types the dashboard understands (protocol-client.md, consumed messages).
/// Anything else is counted in `unknownTypes`.
const KNOWN_SESSION_TYPES: &[&str] = &[
    "rpc_error",
    "list_agents_response",
    "agent_update",
    "agent_status",
    "agent_archived",
    "agent_deleted",
    "agent_stream",
    "agent_timeline_reset",
    "agent_ui_request",
    "agent_ui_resolved",
    "agent_ui_list_response",
    "agent_session_stats_response",
    "fetch_agent_timeline_response",
    "agent_ui_respond_response",
    "interrupt_response",
    "daemon_status_response",
    "daemon_status_update",
    "daemon_status_subscribe_response",
    "daemon_status_unsubscribe_response",
    "daemon_logs_subscribe_response",
    "daemon_logs_unsubscribe_response",
    "daemon_log_lines",
];
const CLOSE_GRACE: Duration = Duration::from_secs(1);

pub(crate) type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// What the entry should connect to.
#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub(crate) entry: DaemonEntry,
    /// Offered as the `pi-studio.bearer.<pw>` subprotocol; never logged.
    pub(crate) password: Option<Secret>,
}

pub(crate) enum Ctl {
    Connect(Target),
    Disconnect,
    Restart(Target),
}

/// Why a session stopped without an error of its own.
pub(crate) enum Ended {
    Disconnect,
    Restart(Box<Target>),
    /// `auth_failed` / `host_rejected`: wait for the user.
    Terminal,
    Gone,
}

pub(crate) struct Task {
    pub(crate) ctl: mpsc::UnboundedReceiver<Ctl>,
    pub(crate) em: Emitter,
    app_version: String,
    ping_every: Duration,
    reset_after_live: Duration,
    pub(crate) counters: Counters,
    last_diagnostics: Instant,
    /// Events produced while handling a frame; flushed by the read loop.
    pub(crate) out: Vec<DaemonEventKind>,
    pub(crate) rpc: Rpc,
    /// `Some` while syncing: known pushes wait here until the `Snapshot` is out.
    pub(crate) buffer: Option<PushBuffer>,
    /// Completed sync requests, consumed by the sync loop.
    pub(crate) sync_done: Vec<(SyncKind, Result<serde_json::Value, SyncFailure>)>,
    pub(crate) stats: StatsExec,
    pub(crate) timeline: TimelineExec,
    pub(crate) shared: Arc<Shared>,
}

impl Task {
    pub(crate) fn new(
        ctl: mpsc::UnboundedReceiver<Ctl>,
        em: Emitter,
        app_version: String,
        ping_every: Duration,
        reset_after_live: Duration,
        shared: Arc<Shared>,
    ) -> Self {
        let now = Instant::now();
        let entry_id = em.entry_id().to_owned();
        // The first diagnostics may go out immediately; later ones are 1 s apart.
        Self {
            ctl,
            em,
            app_version,
            ping_every,
            reset_after_live,
            counters: Counters::new(now),
            last_diagnostics: now.checked_sub(DIAGNOSTICS_EVERY).unwrap_or(now),
            out: Vec::new(),
            rpc: Rpc::new(&entry_id),
            shared,
            buffer: None,
            sync_done: Vec::new(),
            stats: StatsExec::default(),
            timeline: TimelineExec::default(),
        }
    }
}

/// `host:port`, bracketing IPv6 literals.
pub(crate) fn authority(entry: &DaemonEntry) -> String {
    if entry.host.contains(':') { format!("[{}]:{}", entry.host, entry.port) } else { format!("{}:{}", entry.host, entry.port) }
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

impl Task {
    /// The entry's lifetime: idle until told to connect, then sessions until removed.
    pub(crate) async fn run(mut self) {
        let mut target: Option<Target> = None;
        loop {
            let current = match target.take() {
                Some(t) => t,
                None => match self.wait_for_target().await {
                    Some(t) => t,
                    None => return,
                },
            };
            match self.session(&current).await {
                Ended::Restart(next) => target = Some(*next),
                Ended::Disconnect => self.state(ConnState::Idle, None, None).await,
                Ended::Terminal => match self.wait_for_target_in_terminal().await {
                    Some(t) => target = Some(t),
                    None => return,
                },
                Ended::Gone => return,
            }
        }
    }

    async fn wait_for_target(&mut self) -> Option<Target> {
        loop {
            match self.ctl.recv().await? {
                Ctl::Connect(t) | Ctl::Restart(t) => return Some(t),
                Ctl::Disconnect => {}
            }
        }
    }

    /// In `auth_failed` / `host_rejected` a disconnect returns to `idle`; a connect retries.
    async fn wait_for_target_in_terminal(&mut self) -> Option<Target> {
        loop {
            match self.ctl.recv().await? {
                Ctl::Connect(t) | Ctl::Restart(t) => return Some(t),
                Ctl::Disconnect => self.state(ConnState::Idle, None, None).await,
            }
        }
    }

    async fn state(&mut self, state: ConnState, last_error: Option<&ConnError>, next_retry_in_ms: Option<u64>) {
        // Transitions only; never URLs with credentials, passwords or frame bodies.
        log::info!("daemon entry {} -> {}{}", self.em.entry_id(), state.as_str(), last_error.map_or(String::new(), |e| format!(" ({})", e.kind.code())));
        self.em.emit(DaemonEventKind::ConnState { state, since: now_ms(), last_error: last_error.map(ConnError::view), next_retry_in_ms }).await;
    }

    /// Emit `Diagnostics` if a counter changed, at most once per second.
    async fn flush_diagnostics(&mut self) {
        if self.counters.is_dirty() && self.last_diagnostics.elapsed() >= DIAGNOSTICS_EVERY {
            self.last_diagnostics = Instant::now();
            let snapshot = self.counters.take_snapshot(self.last_diagnostics);
            self.em.emit(DaemonEventKind::Diagnostics(snapshot)).await;
        }
    }

    /// Sleep through a backoff, still publishing counters that changed.
    async fn backoff_wait(&mut self, delay: Duration) -> Result<(), Ended> {
        let end = Instant::now() + delay;
        loop {
            let wake = if self.counters.is_dirty() { end.min(self.last_diagnostics + DIAGNOSTICS_EVERY) } else { end };
            self.run_or_interrupt(tokio::time::sleep_until(wake)).await?;
            self.flush_diagnostics().await;
            if Instant::now() >= end {
                return Ok(());
            }
        }
    }

    /// Await `f`, but yield early on a control message. A repeated `Connect` is a no-op.
    pub(crate) async fn run_or_interrupt<F: Future>(&mut self, f: F) -> Result<F::Output, Ended> {
        tokio::pin!(f);
        loop {
            tokio::select! {
                biased;
                out = &mut f => return Ok(out),
                msg = self.ctl.recv() => match msg {
                    None => return Err(Ended::Gone),
                    Some(Ctl::Connect(_)) => {}
                    Some(Ctl::Disconnect) => return Err(Ended::Disconnect),
                    Some(Ctl::Restart(t)) => return Err(Ended::Restart(Box::new(t))),
                },
            }
        }
    }

    async fn session(&mut self, target: &Target) -> Ended {
        match self.cycles(target).await {
            Ok(never) => match never {},
            Err(ended) => ended,
        }
    }

    /// Probe, connect, handshake, live, back off; forever, until interrupted.
    async fn cycles(&mut self, target: &Target) -> Result<std::convert::Infallible, Ended> {
        let entry = &target.entry;
        let mut attempt: u32 = 0;
        let mut probe_gate_used = false;
        let mut handshake_failures: u32 = 0;
        let mut first = true;
        let mut last_error: Option<ConnError> = None;
        loop {
            self.em.begin_attempt();
            if !std::mem::take(&mut first) {
                self.counters.reconnect();
            }
            self.state(ConnState::Probing, last_error.as_ref(), None).await;
            let reachable = self.run_or_interrupt(probe::probe(entry)).await?;
            self.em.emit(DaemonEventKind::Health { reachable, at: now_ms() }).await;
            let failure = if !reachable && !probe_gate_used {
                // One failed probe sends us to backoff; the next cycle tries the socket anyway.
                probe_gate_used = true;
                Some(ConnError::new(ConnErrorKind::Unreachable, "health probe failed"))
            } else {
                if reachable {
                    probe_gate_used = false;
                }
                match self.attempt(target, &mut last_error, &mut attempt, &mut handshake_failures).await? {
                    Attempt::Terminal => return Err(Ended::Terminal),
                    Attempt::Failed(e) => Some(e),
                }
            };
            if let Some(mut error) = failure {
                if handshake_failures >= HANDSHAKE_REJECTED_AFTER {
                    error =
                        ConnError::new(ConnErrorKind::HandshakeRejected, format!("{} consecutive handshake failures: {}", handshake_failures, error.message));
                    attempt = attempt.max(CAP_ATTEMPT);
                }
                let delay = backoff::delay(attempt, backoff::random_unit());
                attempt = attempt.saturating_add(1);
                self.state(ConnState::Backoff, Some(&error), Some(u64::try_from(delay.as_millis()).unwrap_or(u64::MAX))).await;
                last_error = Some(error);
                self.flush_diagnostics().await;
                self.backoff_wait(delay).await?;
            }
        }
    }

    /// `connecting` through `live`. Returns the failure that ended it.
    async fn attempt(
        &mut self,
        target: &Target,
        last_error: &mut Option<ConnError>,
        attempt: &mut u32,
        handshake_failures: &mut u32,
    ) -> Result<Attempt, Ended> {
        let entry = &target.entry;
        self.state(ConnState::Connecting, last_error.as_ref(), None).await;
        let mut ws = match self.run_or_interrupt(open(entry, target.password.as_ref())).await? {
            Ok(ws) => ws,
            Err(e) => return self.classify(e, &entry.host).await,
        };
        self.state(ConnState::Handshaking, last_error.as_ref(), None).await;
        let app_version = self.app_version.clone();
        let info = match self.run_or_interrupt(handshake(&mut ws, &app_version)).await? {
            Ok(info) => info,
            Err(e) => {
                *handshake_failures += 1;
                return Ok(Attempt::Failed(e));
            }
        };
        *handshake_failures = 0;
        self.em.set_server_id(info.server_id.clone());
        self.shared.set_server_id(&info.server_id);
        let caps = info.caps();
        let hello = DaemonEventKind::Hello {
            info: ServerInfoView {
                hostname: info.hostname.clone(),
                version: info.version.clone(),
                features: info.features.iter().filter(|(_, v)| **v == serde_json::Value::Bool(true)).map(|(k, _)| k.clone()).collect(),
            },
            caps: ServerCaps { daemon_observability: caps.daemon_observability },
            degraded: caps
                .degraded()
                .into_iter()
                .map(|d| match d {
                    pistudiodashboard_protocol::DegradedReason::NoObservability => DegradedReason::NoObservability,
                })
                .collect::<BTreeSet<_>>(),
            tls_in_use: entry.tls,
            is_loopback: is_loopback(&entry.host),
        };
        self.em.emit(hello).await;
        self.state(ConnState::Syncing, None, None).await;
        let synced = match self.sync(&mut ws).await {
            Ok(Ok(synced)) => synced,
            Ok(Err(error)) => {
                self.buffer = None;
                return Ok(Attempt::Failed(error));
            }
            Err(ended) => {
                self.buffer = None;
                return Err(ended);
            }
        };
        *last_error = None;
        self.state(ConnState::Live, synced.warning.as_ref(), None).await;
        self.em.emit(DaemonEventKind::Snapshot { agents: synced.agents, ui_pending: synced.ui_pending }).await;
        self.replay_buffered().await;
        self.reset_executors();
        let live_since = Instant::now();
        self.counters.connected(now_ms());
        let mut commands = self.shared.publish();
        let outcome = self.live(&mut ws, &mut commands).await;
        self.shared.withdraw();
        self.fail_all(&mut commands).await;
        self.counters.disconnected();
        if live_since.elapsed() >= self.reset_after_live {
            *attempt = 0;
        }
        match outcome {
            Ok(error) => Ok(Attempt::Failed(error)),
            Err(ended) => {
                close_normally(&mut ws).await;
                Err(ended)
            }
        }
    }

    /// Map an upgrade failure; 401/403 are terminal (D25), everything else backs off.
    async fn classify(&mut self, e: OpenError, host: &str) -> Result<Attempt, Ended> {
        let (kind, terminal_state, message) = match &e {
            OpenError::Status(401) => (ConnErrorKind::AuthFailed, Some(ConnState::AuthFailed), "daemon rejected the password (HTTP 401)".to_owned()),
            OpenError::Status(403) => (
                ConnErrorKind::HostRejected,
                Some(ConnState::HostRejected),
                format!("daemon rejected host `{host}` (HTTP 403); add it to PI_STUDIO_HOSTNAMES or connect by IP"),
            ),
            OpenError::Timeout => (ConnErrorKind::Timeout, None, e.to_string()),
            OpenError::Status(_) | OpenError::Other(_) => (ConnErrorKind::Unreachable, None, e.to_string()),
        };
        let error = ConnError::new(kind, message);
        if let Some(state) = terminal_state {
            self.state(state, Some(&error), None).await;
            return Ok(Attempt::Terminal);
        }
        Ok(Attempt::Failed(error))
    }

    /// The read loop. `Ok(error)` when the connection ended on its own; `Err` on a control message.
    async fn live(&mut self, ws: &mut Ws, commands: &mut mpsc::Receiver<Command>) -> Result<ConnError, Ended> {
        let mut ping = interval(self.ping_every);
        let mut diagnostics = interval(DIAGNOSTICS_EVERY);
        let mut pinger = Pinger::default();
        loop {
            let deadline = self.rpc.next_deadline();
            let event = self.run_or_interrupt(next_event(ws, &mut ping, &mut diagnostics, commands, deadline)).await?;
            match event {
                LiveEvent::Diagnostics => self.flush_diagnostics().await,
                LiveEvent::Deadline => {
                    for kind in self.rpc.expire(Instant::now()) {
                        self.finish(kind, None, MutationResult::Timeout);
                    }
                }
                LiveEvent::Command(cmd) => {
                    if let Some(error) = self.send_command(ws, cmd).await {
                        return Ok(error);
                    }
                }
                LiveEvent::Ping => match pinger.on_tick(Instant::now(), now_ms()) {
                    Tick::Dead => return Ok(ConnError::new(ConnErrorKind::Timeout, "two consecutive pings went unanswered")),
                    Tick::Send(text) => {
                        if let Err(e) = ws.send(Message::text(text)).await {
                            return Ok(ConnError::new(ConnErrorKind::Closed, e.to_string()));
                        }
                    }
                },
                LiveEvent::Frame(None | Some(Ok(Message::Close(_)))) => return Ok(ConnError::new(ConnErrorKind::Closed, "closed by daemon")),
                LiveEvent::Frame(Some(Ok(Message::Text(text)))) => {
                    self.counters.message(Instant::now());
                    if let Some(too_large) = self.on_text(text.as_str(), &mut pinger) {
                        return Ok(too_large);
                    }
                }
                LiveEvent::Frame(Some(Ok(Message::Binary(_)))) => {
                    self.counters.message(Instant::now());
                    self.counters.binary_ignored();
                }
                LiveEvent::Frame(Some(Ok(_))) => {}
                LiveEvent::Frame(Some(Err(WsError::Capacity(_)))) => {
                    self.counters.frame_too_large();
                    return Ok(ConnError::new(ConnErrorKind::FrameTooLarge, format!("frame over {MAX_FRAME_BYTES} bytes")));
                }
                LiveEvent::Frame(Some(Err(e))) => return Ok(ConnError::new(ConnErrorKind::Closed, e.to_string())),
            }
            self.flush_out().await;
            if let Some(error) = self.pump(ws).await {
                return Ok(error);
            }
        }
    }

    /// Register and send one accepted request. `Some(error)` if the socket is gone.
    async fn send_command(&mut self, ws: &mut Ws, cmd: Command) -> Option<ConnError> {
        if let Some(result) = self.start_special(ws, &cmd.request).await {
            return result.err();
        }
        let timeout = timeout_for(&cmd.request);
        let kind = Kind::of(&cmd.request, cmd.token);
        let id = self.rpc.start(kind, timeout);
        match ws.send(Message::text(encode(&cmd.request, &id))).await {
            Ok(()) => None,
            Err(e) => {
                // `fail_all` reports the pending mutation (including this one) as lost.
                Some(ConnError::new(ConnErrorKind::Closed, e.to_string()))
            }
        }
    }

    /// A pending request ended. Mutations become a `RequestResult`; reads are dropped for now.
    pub(crate) fn finish(&mut self, kind: Kind, message: Option<&serde_json::Value>, transport: MutationResult) {
        match kind {
            Kind::Read => self.shared.finished(),
            Kind::Mutation { op, token } => {
                self.shared.finished();
                let result = message.map_or(transport, |m| mutation_outcome(op, m));
                self.out.push(DaemonEventKind::RequestResult { token, op, result });
            }
            Kind::Stats { agent_id } => self.finish_stats(&agent_id, message),
            Kind::Timeline { job, first } => {
                if first {
                    self.shared.finished();
                }
                self.finish_timeline(job, first, message);
            }
            Kind::Sync(which) => {
                let outcome = match message {
                    Some(m) if m.get("type").and_then(serde_json::Value::as_str) == Some("rpc_error") => Err(SyncFailure::Rejected),
                    Some(m) => Ok(m.clone()),
                    None => Err(SyncFailure::TimedOut),
                };
                self.sync_done.push((which, outcome));
            }
        }
    }

    /// The connection is gone: every pending and queued mutation is lost, never replayed (D27).
    async fn fail_all(&mut self, commands: &mut mpsc::Receiver<Command>) {
        for kind in self.rpc.drain() {
            self.finish(kind, None, MutationResult::ConnectionLost);
        }
        while let Ok(cmd) = commands.try_recv() {
            self.finish(Kind::of(&cmd.request, cmd.token), None, MutationResult::ConnectionLost);
        }
        self.flush_out().await;
    }

    /// Classify one text frame into counters and RTT. `Some(error)` ends the connection.
    pub(crate) fn on_text(&mut self, text: &str, pinger: &mut Pinger) -> Option<ConnError> {
        match decode_inbound(text) {
            Err(FrameError::TooLarge { bytes }) => {
                self.counters.frame_too_large();
                return Some(ConnError::new(ConnErrorKind::FrameTooLarge, format!("frame of {bytes} bytes")));
            }
            Err(_) | Ok(Inbound::Malformed { .. }) => self.counters.malformed(),
            Ok(Inbound::Pong(Pong { request_id, .. })) => {
                if let Some(ms) = pinger.on_pong(&request_id, Instant::now()) {
                    self.counters.rtt(ms);
                    self.out.push(DaemonEventKind::Rtt { ms });
                } else {
                    // The answer to a `Ping` request, or a stray pong.
                    self.route(&request_id, &serde_json::Value::Null);
                }
            }
            Ok(Inbound::RpcError(e)) => {
                let message = serde_json::json!({"type": "rpc_error", "requestId": e.request_id, "code": e.code, "message": e.message});
                self.route(&e.request_id, &message);
            }
            Ok(Inbound::Unknown { type_ }) => self.counters.unknown_type(&type_),
            Ok(Inbound::Session { message }) => {
                let answered = message.get("requestId").and_then(serde_json::Value::as_str).is_some_and(|id| self.route(id, &message));
                if !answered {
                    match message.get("type").and_then(serde_json::Value::as_str) {
                        Some(t) if PUSH_TYPES.contains(&t) => self.on_push(message),
                        Some(t) if KNOWN_SESSION_TYPES.contains(&t) => {}
                        Some(t) => self.counters.unknown_type(t),
                        None => self.counters.malformed(),
                    }
                }
            }
            // Typed handling of the rest arrives with the sync/stream tasks.
            Ok(_) => {}
        }
        None
    }

    /// Hand a message carrying `request_id` to its pending request, or count it as late.
    /// `false` when the id is neither pending nor timed out (the message is a push).
    fn route(&mut self, request_id: &str, message: &serde_json::Value) -> bool {
        if let Some(kind) = self.rpc.complete(request_id) {
            self.finish(kind, Some(message), MutationResult::Timeout);
            true
        } else if self.rpc.take_late(request_id) {
            self.counters.late_response();
            true
        } else {
            false
        }
    }
}

enum LiveEvent {
    Frame(Option<Result<Message, WsError>>),
    Ping,
    Diagnostics,
    Command(Command),
    Deadline,
}

fn interval(every: Duration) -> Interval {
    let mut i = tokio::time::interval_at(Instant::now() + every, every);
    i.set_missed_tick_behavior(MissedTickBehavior::Delay);
    i
}

async fn next_event(
    ws: &mut Ws,
    ping: &mut Interval,
    diagnostics: &mut Interval,
    commands: &mut mpsc::Receiver<Command>,
    deadline: Option<Instant>,
) -> LiveEvent {
    let due = async {
        match deadline {
            Some(d) => tokio::time::sleep_until(d).await,
            None => std::future::pending().await,
        }
    };
    // Data first: a frame that is ready wins over a timer that expired in the same wake-up
    // (the tokio coop budget still lets timers run under a flood).
    tokio::select! {
        biased;
        frame = ws.next() => LiveEvent::Frame(frame),
        Some(cmd) = commands.recv() => LiveEvent::Command(cmd),
        () = due => LiveEvent::Deadline,
        _ = ping.tick() => LiveEvent::Ping,
        _ = diagnostics.tick() => LiveEvent::Diagnostics,
    }
}

enum Attempt {
    Failed(ConnError),
    Terminal,
}

#[derive(Debug)]
enum OpenError {
    Status(u16),
    Timeout,
    Other(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Status(code) => write!(f, "upgrade refused with HTTP {code}"),
            Self::Timeout => f.write_str("connect timed out"),
            Self::Other(m) => f.write_str(m),
        }
    }
}

fn ws_config() -> WebSocketConfig {
    WebSocketConfig::default().max_message_size(Some(MAX_FRAME_BYTES)).max_frame_size(Some(MAX_FRAME_BYTES))
}

/// TCP + (TLS) + upgrade within 5 s. The URL carries no credentials.
async fn open(entry: &DaemonEntry, password: Option<&Secret>) -> Result<Ws, OpenError> {
    let scheme = if entry.tls { "wss" } else { "ws" };
    let url = format!("{scheme}://{}/ws", authority(entry));
    let connector = if entry.tls { Some(Connector::Rustls(tls::client_config().map_err(OpenError::Other)?)) } else { None };
    let mut request = url.into_client_request().map_err(|e| OpenError::Other(e.to_string()))?;
    if let Some(pw) = password {
        let value = HeaderValue::from_str(&format!("{BEARER_PREFIX}{}", pw.expose()))
            .map_err(|_| OpenError::Other("password is not a valid subprotocol token".to_owned()))?;
        request.headers_mut().insert("Sec-WebSocket-Protocol", value);
    }
    let connect = tokio_tungstenite::connect_async_tls_with_config(request, Some(ws_config()), true, connector);
    match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
        Err(_) => Err(OpenError::Timeout),
        Ok(Ok((ws, _response))) => Ok(ws),
        Ok(Err(WsError::Http(response))) => Err(OpenError::Status(response.status().as_u16())),
        Ok(Err(e)) => Err(OpenError::Other(e.to_string())),
    }
}

/// Send `hello`, then wait up to 5 s for the first text frame, which must be a usable `server_info`.
async fn handshake(ws: &mut Ws, app_version: &str) -> Result<ServerInfo, ConnError> {
    let client_id = new_client_id();
    let hello = Hello::new(client_id, app_version).encode();
    ws.send(Message::text(hello)).await.map_err(|e| ConnError::new(ConnErrorKind::Closed, e.to_string()))?;
    let first = tokio::time::timeout(SERVER_INFO_TIMEOUT, async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                other => return other,
            }
        }
    })
    .await
    .map_err(|_| ConnError::new(ConnErrorKind::Timeout, "no server_info within 5 s"))?;
    match first {
        Some(Ok(Message::Text(text))) => match decode_inbound(text.as_str()) {
            Ok(Inbound::ServerInfo(info)) => Ok(info),
            Ok(Inbound::BadHandshake(e)) => Err(ConnError::new(ConnErrorKind::BadHandshake, e.to_string())),
            _ => Err(ConnError::new(ConnErrorKind::BadHandshake, "first frame is not status/server_info")),
        },
        Some(Ok(Message::Close(_))) | None => Err(ConnError::new(ConnErrorKind::Closed, "closed during handshake")),
        Some(Ok(_)) => Err(ConnError::new(ConnErrorKind::BadHandshake, "first frame is not text")),
        Some(Err(e)) => Err(ConnError::new(ConnErrorKind::Closed, e.to_string())),
    }
}

/// A random per-connection id (UUIDv4 text); not persisted.
fn new_client_id() -> String {
    let mut b = [0_u8; 16];
    // On OS RNG failure the all-zero-ish id is still unique enough per process start; the
    // daemon only uses it to label the connection.
    let _ = getrandom::fill(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", h[0..4].concat(), h[4..6].concat(), h[6..8].concat(), h[8..10].concat(), h[10..16].concat())
}

/// Close with code 1000 and give the peer a moment to acknowledge.
async fn close_normally(ws: &mut Ws) {
    let frame = CloseFrame { code: CloseCode::Normal, reason: "".into() };
    let _ = tokio::time::timeout(CLOSE_GRACE, async {
        let _ = ws.send(Message::Close(Some(frame))).await;
        while let Some(Ok(_)) = ws.next().await {}
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_looks_like_a_uuid_v4() {
        let id = new_client_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
        assert_eq!(id.as_bytes()[14], b'4');
    }

    #[test]
    fn loopback_detection() {
        for h in ["127.0.0.1", "127.9.9.9", "::1", "localhost", "LocalHost"] {
            assert!(is_loopback(h), "{h}");
        }
        for h in ["192.168.1.2", "example.com", "::2"] {
            assert!(!is_loopback(h), "{h}");
        }
    }
}
