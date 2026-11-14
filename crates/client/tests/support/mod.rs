//! Shared test support: a scriptable in-process pi-studio daemon.
#![allow(dead_code)]

pub mod fake_daemon;

use std::time::Duration;

use pistudiodashboard_client::{DaemonEvent, DaemonEventKind};
use pistudiodashboard_config::DaemonEntry;
use pistudiodashboard_model::ConnState;
use tokio::sync::mpsc;

/// A loopback entry for `port`.
pub fn entry(id: &str, port: u16) -> DaemonEntry {
    DaemonEntry {
        entry_id: id.to_owned(),
        name: id.to_owned(),
        host: "127.0.0.1".to_owned(),
        port,
        tls: false,
        web_ui_url: None,
        last_seen_server_id: None,
        auto_detected: false,
        extra: Default::default(),
    }
}

/// A port nothing listens on.
pub async fn closed_port() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    l.local_addr().expect("addr").port()
}

/// The event receiver plus the events of a batch that came after the last match.
pub struct Events {
    rx: mpsc::Receiver<Vec<DaemonEvent>>,
    stash: std::collections::VecDeque<DaemonEvent>,
}

impl Events {
    pub fn new(rx: mpsc::Receiver<Vec<DaemonEvent>>) -> Self {
        Self { rx, stash: std::collections::VecDeque::new() }
    }

    /// Everything immediately available; `Err` when nothing is.
    pub fn try_recv(&mut self) -> Result<Vec<DaemonEvent>, ()> {
        let mut out: Vec<DaemonEvent> = self.stash.drain(..).collect();
        while let Ok(batch) = self.rx.try_recv() {
            out.extend(batch);
        }
        if out.is_empty() { Err(()) } else { Ok(out) }
    }

    /// The next batch (stashed events first); `None` when the channel is closed.
    pub async fn recv(&mut self) -> Option<Vec<DaemonEvent>> {
        if !self.stash.is_empty() {
            return Some(self.stash.drain(..).collect());
        }
        self.rx.recv().await
    }

    async fn next(&mut self) -> DaemonEvent {
        loop {
            if let Some(e) = self.stash.pop_front() {
                return e;
            }
            let batch = tokio::time::timeout(Duration::from_secs(600), self.rx.recv()).await.expect("event timeout").expect("channel closed");
            self.stash.extend(batch);
        }
    }
}

/// Receive events until `done` returns true for one of them; returns all seen, in order.
/// Events after the match stay available for the next call.
pub async fn collect_until(rx: &mut Events, mut done: impl FnMut(&DaemonEvent) -> bool) -> Vec<DaemonEvent> {
    let mut seen = Vec::new();
    loop {
        let e = rx.next().await;
        let stop = done(&e);
        seen.push(e);
        if stop {
            return seen;
        }
    }
}

/// Receive events until `n` of them satisfy `pred`; returns all seen.
pub async fn collect_count(rx: &mut Events, n: usize, pred: impl Fn(&DaemonEvent) -> bool) -> Vec<DaemonEvent> {
    let mut seen = Vec::new();
    let mut hits = 0;
    while hits < n {
        let e = rx.next().await;
        hits += usize::from(pred(&e));
        seen.push(e);
    }
    seen
}

/// The full wire frame (as text) of a vendored fixture: `dir` is `pi-studio` or `local`.
pub fn fixture_frame(dir: &str, name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../protocol/tests/fixtures").join(dir).join(name);
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
    v["frame"].to_string()
}

pub fn state_of(e: &DaemonEvent) -> Option<ConnState> {
    match &e.kind {
        DaemonEventKind::ConnState { state, .. } => Some(*state),
        _ => None,
    }
}

pub fn is_state(e: &DaemonEvent, s: ConnState) -> bool {
    state_of(e) == Some(s)
}
