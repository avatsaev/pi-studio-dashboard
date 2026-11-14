//! D29: neither events nor log output may contain the password or the bearer subprotocol.
//!
//! Logs are captured at DEBUG for every target. tungstenite logs the full upgrade request,
//! including `Sec-WebSocket-Protocol`, at TRACE: the app's logger must never enable TRACE for
//! `tungstenite`/`tokio_tungstenite` (sprint-005 logger setup).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Mutex;
use std::time::Duration;

use log::{Log, Metadata, Record};
use pistudiodashboard_client::{ClientConfig, ClientHandle, Secret};
use pistudiodashboard_model::ConnState;
use support::fake_daemon::{Behavior, FakeDaemon};
use support::{collect_until, entry, is_state};

struct Capture(Mutex<Vec<String>>);

impl Log for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn log(&self, r: &Record<'_>) {
        self.0.lock().unwrap().push(format!("{} {}: {}", r.level(), r.target(), r.args()));
    }
    fn flush(&self) {}
}

static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));

#[tokio::test(start_paused = true)]
async fn no_secret_in_events_or_logs_across_connect_auth_failure_and_reconnect() {
    log::set_logger(&CAPTURE).unwrap();
    log::set_max_level(log::LevelFilter::Debug);
    let daemon = FakeDaemon::start(Behavior { require_subprotocol: Some("pi-studio.bearer.hunter2pw".into()), ..Behavior::default() }).await;
    let (client, rx) = ClientHandle::start_in(tokio::runtime::Handle::current(), ClientConfig::new("0.1.0"));
    let mut rx = support::Events::new(rx);
    let mut events = Vec::new();

    client.connect(entry("e1", daemon.port()), Some(Secret::new("wrongpw99"))).unwrap();
    events.extend(collect_until(&mut rx, |e| is_state(e, ConnState::AuthFailed)).await);
    client.connect(entry("e1", daemon.port()), Some(Secret::new("hunter2pw"))).unwrap();
    events.extend(collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await);
    // Force a reconnect with the password configured.
    daemon.set(|b| b.upgrade_status = Some(500));
    client.restart("e1", entry("e1", daemon.port()), Some(Secret::new("hunter2pw"))).unwrap();
    events.extend(collect_until(&mut rx, |e| is_state(e, ConnState::Backoff)).await);
    tokio::time::sleep(Duration::from_secs(3)).await;
    if let Ok(rest) = rx.try_recv() {
        events.extend(rest);
    }

    let events_text = serde_json::to_string(&events).unwrap();
    let logs = CAPTURE.0.lock().unwrap().join("\n");
    assert!(logs.contains("daemon entry e1"), "transitions are logged: {logs}");
    for secret in ["hunter2pw", "wrongpw99", "pi-studio.bearer."] {
        assert!(!events_text.contains(secret), "{secret} leaked into events");
        assert!(!logs.contains(secret), "{secret} leaked into logs:\n{logs}");
    }
}
