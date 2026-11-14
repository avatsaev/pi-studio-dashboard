//! The whole app pipeline against an in-process fake daemon: real client, feed pump, engine,
//! shell and owner thread (no window).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../../../crates/client/tests/support/mod.rs"]
mod support;

use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pistudiodashboard_client::OutboundRequest;
use pistudiodashboard_engine::{EngineServices, SendError};
use pistudiodashboard_shell::{Envelope, LocalDaemon, Request};
use serde_json::{Value, json};
use support::fake_daemon::{AgentsReply, Behavior, FakeDaemon};

use crate::io_worker::IoWorker;
use crate::keychain::Store;
use crate::owner::{self, Wiring};
use crate::runtime;

struct NoStore;

impl Store for NoStore {
    fn get(&self, _: &str) -> Result<Option<String>, pistudiodashboard_engine::KeychainError> {
        Err(pistudiodashboard_engine::KeychainError::Unavailable)
    }
    fn set(&self, _: &str, _: &str) -> Result<(), pistudiodashboard_engine::KeychainError> {
        Err(pistudiodashboard_engine::KeychainError::Unavailable)
    }
    fn delete(&self, _: &str) -> Result<(), pistudiodashboard_engine::KeychainError> {
        Ok(())
    }
}

struct Rig {
    requests: Sender<Envelope>,
    views: Arc<Mutex<Vec<Value>>>,
    client: Arc<pistudiodashboard_client::ClientHandle>,
    _dir: tempfile::TempDir,
}

impl Rig {
    /// Start everything; `detected` is the daemon startup finds (`None`: an unused port).
    fn start(detected: LocalDaemon) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (requests, inbox) = channel();
        let worker = Arc::new(IoWorker::start(Some(dir.path().to_owned()), requests.clone(), Arc::new(NoStore)));
        let client = runtime::start_client(requests.clone());
        let wiring = Wiring { client: Arc::clone(&client), worker, config_dir: Some(dir.path().to_owned()) };
        let views: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = Arc::clone(&views);
        std::thread::spawn(move || {
            let app = owner::build_with(&wiring, &detected);
            owner::run(&inbox, app, |v| sink.lock().unwrap().push(v.clone()));
        });
        Self { requests, views, client, _dir: dir }
    }

    fn call(&self, method: &str, params: Value) -> Value {
        let (envelope, reply) = Envelope::new(Request::Control { method: method.into(), params });
        self.requests.send(envelope).unwrap();
        reply.recv_timeout(Duration::from_secs(10)).unwrap()
    }

    fn internal(&self, command: &str, params: Value) -> Value {
        let (envelope, reply) = Envelope::new(Request::Internal { command: command.into(), params });
        self.requests.send(envelope).unwrap();
        reply.recv_timeout(Duration::from_secs(10)).unwrap()
    }

    fn wait_for(&self, what: &str, mut ok: impl FnMut(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let view = self.call("ui.inspect", Value::Null)["result"].clone();
            if ok(&view) {
                return view;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}: {view}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn local(port: u16) -> LocalDaemon {
    LocalDaemon { name: "Local".into(), host: "127.0.0.1".into(), port, tls: false, web_ui_url: None }
}

fn agent(id: &str, status: &str) -> Value {
    json!({"agentId": id, "status": status, "title": format!("t-{id}"), "labels": {}})
}

/// The vendored `status/server_info` frame with a `serverId` (the support module's own lookup is
/// relative to the client crate).
fn server_info(server_id: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../crates/protocol/tests/fixtures/pi-studio/status.server_info.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut frame = fixture["frame"].clone();
    frame["payload"]["serverId"] = Value::from(server_id);
    frame.to_string()
}

fn runtime_for_daemon() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap()
}

#[test]
fn the_detected_daemon_connects_its_agents_appear_and_a_stats_request_reaches_it() {
    let rt = runtime_for_daemon();
    let daemon = rt.block_on(FakeDaemon::start(Behavior {
        handshake: support::fake_daemon::Handshake::Raw(server_info("00000000-0000-4000-8000-0000000000aa")),
        agents: AgentsReply::Agents(vec![agent("a1", "running"), agent("a2", "idle")]),
        ..Behavior::default()
    }));
    let rig = Rig::start(local(daemon.port()));
    let view = rig.wait_for("two agents", |v| v["document"]["cards"]["total"] == 2);
    assert_eq!(view["document"]["daemonChips"][0]["name"], "Local");
    assert_eq!(view["document"]["daemonChips"][0]["conn"], "live");
    assert_eq!(view["document"]["daemonChips"][0]["autoDetected"], true);
    // The snapshot asked for stats of the running agent and of the one without usage.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !daemon.received().iter().any(|m| support::fake_daemon::inner_type(m) == "agent_session_stats_request") {
        assert!(Instant::now() < deadline, "no stats request: {:?}", daemon.received());
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!rig.views.lock().unwrap().is_empty(), "a view event was emitted");
    assert_eq!(view["document"]["daemonChips"][0]["hasPassword"], false);
}

#[test]
fn sending_to_an_unknown_or_unconnected_daemon_is_not_connected() {
    let (requests, _inbox) = channel();
    let client = runtime::start_client(requests);
    let mut services = EngineServices::default();
    runtime::install_hooks(&mut services, &client);
    let send = services.send_to_daemon.as_mut().unwrap();
    assert_eq!(send("nope", OutboundRequest::SessionStats { agent_id: "a".into() }), Err(SendError::NotConnected));
}

#[test]
fn a_malformed_batch_is_rejected_and_the_next_one_applies() {
    let rig = Rig::start(local(1));
    let before = rig.internal("feed.ingest", json!({"events": []}));
    assert_eq!(before["ok"], true);
    let bad = rig.internal("feed.ingest", json!({"events": "x"}));
    assert_eq!(bad["ok"], false);
    let revision = |rig: &Rig| rig.call("ui.inspect", Value::Null)["result"]["document"]["revision"].as_u64().unwrap();
    let r1 = revision(&rig);
    assert_eq!(rig.internal("feed.ingest", json!({"events": []}))["ok"], true);
    assert_eq!(revision(&rig), r1 + 1);
}

#[test]
fn a_thousand_batches_in_a_burst_emit_few_views_and_the_last_state_is_emitted() {
    let rig = Rig::start(local(1));
    let started = Instant::now();
    let before = rig.views.lock().unwrap().len();
    let mut replies = Vec::new();
    for _ in 0..1000 {
        let (envelope, reply) = Envelope::new(Request::Internal { command: "feed.ingest".into(), params: json!({"events": []}) });
        rig.requests.send(envelope).unwrap();
        replies.push(reply);
    }
    for r in &replies {
        r.recv_timeout(Duration::from_secs(20)).unwrap();
    }
    let burst = started.elapsed();
    std::thread::sleep(Duration::from_millis(400));
    let views = rig.views.lock().unwrap();
    let emitted = views.len() - before;
    let allowed = usize::try_from(burst.as_millis() / 100).unwrap() + 2;
    assert!(emitted <= allowed.max(10), "{emitted} views for a burst of {burst:?}");
    let last_revision = views.last().unwrap()["document"]["revision"].as_u64().unwrap();
    drop(views);
    let now_revision = rig.call("ui.inspect", Value::Null)["result"]["document"]["revision"].as_u64().unwrap();
    assert_eq!(last_revision, now_revision, "the trailing emit carried the final state");
    drop(rig.client);
}

#[test]
fn a_daemon_that_is_not_there_is_added_but_stays_unconnected() {
    let rig = Rig::start(local(1));
    let view = rig.wait_for("a chip", |v| v["document"]["daemonChips"].as_array().is_some_and(|c| !c.is_empty()));
    assert_ne!(view["document"]["daemonChips"][0]["conn"], "live");
    assert_eq!(view["document"]["cards"]["total"], 0);
}

/// The grouping choice survives a restart: restored from the file, and a change reaches the file.
#[test]
fn the_saved_grouping_is_restored_at_startup_and_a_change_is_saved() {
    use pistudiodashboard_engine::GroupBy;
    let start = |dir: &std::path::Path| {
        let (requests, _inbox) = channel();
        let worker = Arc::new(IoWorker::start(Some(dir.to_owned()), requests.clone(), Arc::new(NoStore)));
        let client = runtime::start_client(requests);
        let wiring = Wiring { client, worker: Arc::clone(&worker), config_dir: Some(dir.to_owned()) };
        (owner::build_with(&wiring, &local(1)), worker)
    };
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    std::fs::write(&file, r#"{"version":1,"settings":{"groupBy":"none","zoom":150}}"#).unwrap();
    let (mut app, worker) = start(dir.path());
    assert_eq!(app.ui.group_by, GroupBy::None, "a saved flat layout comes back flat");
    assert_eq!(app.ui.zoom, 150, "a saved zoom comes back");
    app.run("view.setGroupBy", json!({"groupBy": "workspace"})).unwrap();
    app.run("view.zoomOut", json!({})).unwrap();
    worker.shutdown();
    assert!(
        std::fs::read_to_string(&file).unwrap().contains("\"groupBy\": \"workspace\"") && std::fs::read_to_string(&file).unwrap().contains("\"zoom\": 125")
    );

    let fresh = tempfile::tempdir().unwrap();
    let (grouped, worker) = start(fresh.path());
    assert_eq!((grouped.ui.group_by, grouped.ui.zoom), (GroupBy::Workspace, 100), "no file, no key: grouped by workspace at 100%");
    worker.shutdown();
}
