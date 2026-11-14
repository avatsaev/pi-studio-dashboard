//! In-process fake pi-studio daemon: `GET /api/health` plus a WebSocket endpoint, scriptable
//! per connection through [`Behavior`] (mutable while running).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{HeaderValue, StatusCode};

/// Scripted replies: called with every post-handshake JSON frame except `ping`; the returned
/// text frames are sent back in order.
#[derive(Clone)]
pub struct Responder(pub Arc<ReplyFn>);

type ReplyFn = dyn Fn(&Value) -> Vec<String> + Send + Sync;

impl std::fmt::Debug for Responder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Responder")
    }
}

impl Responder {
    pub fn new(f: impl Fn(&Value) -> Vec<String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }
}

/// The wire type of a frame: the inner type of a `session` envelope, else the frame's own.
pub fn inner_type(v: &Value) -> &str {
    if v["type"] == "session" { v["message"]["type"].as_str().unwrap_or("") } else { v["type"].as_str().unwrap_or("") }
}

/// How the fake answers `list_agents_request` (the first one on each connection).
#[derive(Clone, Debug)]
pub enum AgentsReply {
    /// `list_agents_response` with these agent objects.
    Agents(Vec<Value>),
    Silent,
    /// `rpc_error`.
    Error,
}

/// How the fake answers `agent_ui_list_request` (the first one on each connection).
#[derive(Clone, Debug)]
pub enum UiReply {
    /// `agent_ui_list_response {ok:true, pending}`.
    Pending(Vec<Value>),
    /// `agent_ui_list_response {ok:false, error}`.
    NotOk,
    Silent,
    /// `rpc_error`.
    Error,
}

#[derive(Clone, Debug)]
pub enum HealthMode {
    Ok,
    NotFound,
    /// Close the socket without answering.
    Drop,
}

#[derive(Clone, Debug)]
pub enum Handshake {
    /// Reply to the first client frame with `status/server_info` from the vendored fixture.
    ServerInfo { server_id: Option<String>, features: Option<Value> },
    /// Reply with this exact text frame.
    Raw(String),
    /// Read but never answer.
    Silent,
    /// Close the socket right after reading the first frame.
    Close,
}

/// A frame the daemon pushes right after the handshake.
#[derive(Clone, Debug)]
pub enum Outgoing {
    Text(String),
    Binary(Vec<u8>),
}

#[derive(Clone, Debug)]
pub enum After {
    Stay,
    Close(Duration),
    /// Send a text frame of this many bytes right after the handshake.
    Oversize(usize),
    /// Push these frames right after the handshake, then stay.
    Send(Vec<Outgoing>),
}

#[derive(Clone, Debug)]
pub struct Behavior {
    pub health: HealthMode,
    /// Refuse the upgrade with this HTTP status.
    pub upgrade_status: Option<u16>,
    pub handshake: Handshake,
    pub after: After,
    /// Answer `ping` with `pong` (default true).
    pub pong: bool,
    /// Refuse the upgrade with 401 unless the client offers exactly this subprotocol.
    pub require_subprotocol: Option<String>,
    pub agents: AgentsReply,
    pub ui: UiReply,
    /// Frames pushed right before the `agent_ui_list_response` (the sync race).
    pub push_before_ui_list: Vec<String>,
    pub responder: Option<Responder>,
    /// Close the socket (without answering) when a request of this wire type arrives.
    pub close_on: Option<String>,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            health: HealthMode::Ok,
            upgrade_status: None,
            handshake: Handshake::ServerInfo { server_id: Some("00000000-0000-4000-8000-0000000000aa".into()), features: None },
            after: After::Stay,
            pong: true,
            require_subprotocol: None,
            agents: AgentsReply::Agents(Vec::new()),
            ui: UiReply::Pending(Vec::new()),
            push_before_ui_list: Vec::new(),
            responder: None,
            close_on: None,
        }
    }
}

#[derive(Default)]
struct Shared {
    behavior: Mutex<Behavior>,
    received: Mutex<Vec<String>>,
    subprotocols: Mutex<Vec<String>>,
    close_codes: Mutex<Vec<Option<u16>>>,
    push: Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>,
    upgrades: AtomicUsize,
    health_hits: AtomicUsize,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct FakeDaemon {
    pub addr: SocketAddr,
    shared: Arc<Shared>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for FakeDaemon {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl FakeDaemon {
    pub async fn start(behavior: Behavior) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let shared = Arc::new(Shared::default());
        *lock(&shared.behavior) = behavior;
        let s = shared.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(serve(stream, s.clone()));
            }
        });
        Self { addr, shared, task }
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn set(&self, f: impl FnOnce(&mut Behavior)) {
        f(&mut lock(&self.shared.behavior));
    }

    /// Every text frame received from clients, in order, across connections.
    pub fn received(&self) -> Vec<Value> {
        lock(&self.shared.received).iter().filter_map(|t| serde_json::from_str(t).ok()).collect()
    }

    /// Send a text frame on the most recent connection.
    pub fn push(&self, text: impl Into<String>) {
        if let Some(tx) = lock(&self.shared.push).as_ref() {
            let _ = tx.send(text.into());
        }
    }

    pub fn upgrades(&self) -> usize {
        self.shared.upgrades.load(Ordering::SeqCst)
    }

    pub fn health_hits(&self) -> usize {
        self.shared.health_hits.load(Ordering::SeqCst)
    }

    pub fn close_codes(&self) -> Vec<Option<u16>> {
        lock(&self.shared.close_codes).clone()
    }

    pub fn subprotocols(&self) -> Vec<String> {
        lock(&self.shared.subprotocols).clone()
    }
}

fn behavior(s: &Shared) -> Behavior {
    lock(&s.behavior).clone()
}

async fn serve(stream: TcpStream, s: Arc<Shared>) {
    let mut peek = [0_u8; 32];
    let n = loop {
        match stream.peek(&mut peek).await {
            Ok(n) if n >= 16 => break n,
            Ok(0) | Err(_) => return,
            Ok(_) => tokio::time::sleep(Duration::from_millis(1)).await,
        }
    };
    if peek[..n].starts_with(b"GET /api/health") {
        health(stream, &s).await;
    } else {
        websocket(stream, s).await;
    }
}

async fn health(mut stream: TcpStream, s: &Shared) {
    let mut head = Vec::new();
    let mut buf = [0_u8; 512];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
    }
    s.health_hits.fetch_add(1, Ordering::SeqCst);
    let reply: &[u8] = match behavior(s).health {
        HealthMode::Ok => b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}",
        HealthMode::NotFound => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        HealthMode::Drop => return,
    };
    let _ = stream.write_all(reply).await;
    let _ = stream.shutdown().await;
}

fn server_info_frame(server_id: Option<&str>, features: Option<&Value>) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../protocol/tests/fixtures/pi-studio/status.server_info.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
    let mut frame = fixture["frame"].clone();
    match server_id {
        Some(id) => frame["payload"]["serverId"] = Value::from(id),
        None => {
            if let Some(p) = frame["payload"].as_object_mut() {
                p.remove("serverId");
            }
        }
    }
    if let Some(f) = features {
        frame["payload"]["features"] = f.clone();
    }
    frame.to_string()
}

#[allow(clippy::result_large_err)]
async fn websocket(stream: TcpStream, s: Arc<Shared>) {
    let b = behavior(&s);
    let sp = s.clone();
    let callback = move |req: &Request, mut resp: Response| -> Result<Response, ErrorResponse> {
        if let Some(code) = b.upgrade_status {
            let mut err = ErrorResponse::new(Some(String::new()));
            *err.status_mut() = StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST);
            return Err(err);
        }
        let offered = req.headers().get("Sec-WebSocket-Protocol").and_then(|v| v.to_str().ok());
        if b.require_subprotocol.as_deref().is_some_and(|required| offered != Some(required)) {
            let mut err = ErrorResponse::new(Some(String::new()));
            *err.status_mut() = StatusCode::UNAUTHORIZED;
            return Err(err);
        }
        if let Some(offered) = offered {
            lock(&sp.subprotocols).push(offered.to_owned());
            if let Ok(v) = HeaderValue::from_str(offered) {
                resp.headers_mut().insert("Sec-WebSocket-Protocol", v);
            }
        }
        Ok(resp)
    };
    let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(stream, callback).await else { return };
    s.upgrades.fetch_add(1, Ordering::SeqCst);
    let b = behavior(&s);
    let (push_tx, mut push_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    *lock(&s.push) = Some(push_tx);
    let mut handshaken = false;
    let (mut agents_answered, mut ui_answered) = (false, false);
    let deadline = match b.after {
        After::Close(d) => Some(d),
        _ => None,
    };
    let closer = async {
        match deadline {
            Some(d) => tokio::time::sleep(d).await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(closer);
    loop {
        tokio::select! {
            () = &mut closer => {
                let _ = ws.close(None).await;
                return;
            }
            Some(text) = push_rx.recv() => {
                let _ = ws.send(Message::text(text)).await;
            }
            frame = ws.next() => match frame {
                Some(Ok(Message::Text(t))) => {
                    lock(&s.received).push(t.as_str().to_owned());
                    if !handshaken {
                        handshaken = true;
                        let reply = match &b.handshake {
                            Handshake::ServerInfo { server_id, features } => Some(server_info_frame(server_id.as_deref(), features.as_ref())),
                            Handshake::Raw(t) => Some(t.clone()),
                            Handshake::Silent => None,
                            Handshake::Close => {
                                let _ = ws.close(None).await;
                                return;
                            }
                        };
                        if let Some(r) = reply {
                            let _ = ws.send(Message::text(r)).await;
                        }
                        match &b.after {
                            After::Oversize(n) => {
                                let _ = ws.send(Message::text("x".repeat(*n))).await;
                            }
                            After::Send(frames) => {
                                for f in frames {
                                    let m = match f {
                                        Outgoing::Text(t) => Message::text(t.clone()),
                                        Outgoing::Binary(b) => Message::binary(b.clone()),
                                    };
                                    let _ = ws.send(m).await;
                                }
                            }
                            After::Stay | After::Close(_) => {}
                        }
                    } else if let Some(pong) = b.pong.then(|| pong_for(t.as_str())).flatten() {
                        let _ = ws.send(Message::text(pong)).await;
                    } else if let Ok(frame) = serde_json::from_str::<Value>(t.as_str()) {
                        if b.close_on.as_deref() == Some(inner_type(&frame)) {
                            let _ = ws.close(None).await;
                            return;
                        }
                        for reply in sync_replies(&b, &frame, &mut agents_answered, &mut ui_answered) {
                            let _ = ws.send(Message::text(reply)).await;
                        }
                        if let Some(r) = &b.responder {
                            for reply in (r.0)(&frame) {
                                let _ = ws.send(Message::text(reply)).await;
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(f))) => {
                    lock(&s.close_codes).push(f.map(|f| u16::from(f.code)));
                    return;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return,
            }
        }
    }
}

/// The `pong` answering a `ping` frame, if `text` is one.
fn pong_for(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    if v["type"] != "ping" {
        return None;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    Some(serde_json::json!({"type":"pong","requestId":v["requestId"],"clientSentAt":v["clientSentAt"],"serverReceivedAt":now,"serverSentAt":now}).to_string())
}

/// The scripted answers to the sync-on-connect requests (only the first of each per connection).
fn sync_replies(b: &Behavior, frame: &Value, agents_answered: &mut bool, ui_answered: &mut bool) -> Vec<String> {
    let rid = frame["message"]["requestId"].clone();
    let wrap = |m: Value| json!({"type": "session", "message": m}).to_string();
    let rpc_error = || wrap(json!({"type": "rpc_error", "requestId": rid, "code": "handler_error", "message": "scripted"}));
    match inner_type(frame) {
        "list_agents_request" if !*agents_answered => {
            *agents_answered = true;
            match &b.agents {
                AgentsReply::Agents(agents) => vec![wrap(json!({"type": "list_agents_response", "requestId": rid, "agents": agents}))],
                AgentsReply::Silent => vec![],
                AgentsReply::Error => vec![rpc_error()],
            }
        }
        "agent_ui_list_request" if !*ui_answered => {
            *ui_answered = true;
            let mut out = b.push_before_ui_list.clone();
            match &b.ui {
                UiReply::Pending(p) => {
                    out.push(wrap(json!({"type": "agent_ui_list_response", "requestId": rid, "payload": {"ok": true, "pending": p, "surfaces": []}})))
                }
                UiReply::NotOk => out.push(wrap(json!({"type": "agent_ui_list_response", "requestId": rid, "payload": {"ok": false, "error": "scripted"}}))),
                UiReply::Silent => {}
                UiReply::Error => out.push(rpc_error()),
            }
            out
        }
        _ => vec![],
    }
}
