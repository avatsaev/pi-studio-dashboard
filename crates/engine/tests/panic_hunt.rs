//! Run every command with params nobody asked for. A command must answer with `Ok` or `Err`,
//! never panic or hang. Each call runs on its own thread with a deadline, and a process-wide
//! hook counts panics, because `Session::execute` contains a command's panic and returns an
//! ordinary `Err`. Opt-in locally, always run by the gate:
//! `cargo test -p pistudiodashboard-engine --test panic_hunt -- --ignored`.
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use pistudiodashboard_config::{Config, DaemonEntry};
use pistudiodashboard_engine::{EngineServices, LoadedConfig, Session, command_specs};
use pistudiodashboard_model::{AgentEntry, AgentStatus, Attention, DaemonSlice, DrillBuffer, DrillItem, DrillTarget, MonitorState, TurnKind, UiRequestItem};
use serde_json::Value;

const DEADLINE: Duration = Duration::from_secs(10);
static PANICS: AtomicUsize = AtomicUsize::new(0);

fn fresh() -> Session {
    let mut m = MonitorState::default();
    let mut d = DaemonSlice::new("e1", "box");
    d.server_id = Some("srv".into());
    m.daemons.push(d);
    m.upsert_agent(AgentEntry { server_id: "srv".into(), agent_id: "run".into(), status: AgentStatus::Running, ..AgentEntry::default() });
    m.upsert_agent(AgentEntry {
        server_id: "srv".into(),
        agent_id: "err".into(),
        status: AgentStatus::Error,
        attention: Attention { pending_dialogs: 0, errored: true },
        ..AgentEntry::default()
    });
    m.upsert_ui_request(UiRequestItem { server_id: "srv".into(), request_id: "r1".into(), agent_id: "run".into(), ..UiRequestItem::default() });
    let mut buf = DrillBuffer { target: DrillTarget { server_id: "srv".into(), agent_id: "run".into() }, ..DrillBuffer::default() };
    buf.items.push(DrillItem::Turn { kind: TurnKind::Started });
    m.drilldown = Some(buf);
    let services = EngineServices { daemon_link: Some(Box::new(|_, _| Ok(()))), ..EngineServices::default() };
    let mut s = Session::with_monitor(m).with_services(services);
    // One configured entry (`e1`) so the daemon commands reach their deep paths.
    let entry = DaemonEntry {
        entry_id: "e1".into(),
        name: "box".into(),
        host: "127.0.0.1".into(),
        port: 6790,
        tls: false,
        web_ui_url: None,
        last_seen_server_id: None,
        auto_detected: false,
        extra: Default::default(),
    };
    s.apply_loaded_config(LoadedConfig { config: Config { daemons: vec![entry], ..Config::default() }, issues: Vec::new(), read_only: false });
    let _ = s.update(|m| {
        if let Some(d) = m.daemons.first_mut() {
            d.server_id = Some("srv".into());
        }
        Ok(())
    });
    s
}

/// `None` on a hang, else whether the call finished without any panic in the process.
fn run_watched(id: &'static str, params: Value) -> Option<bool> {
    let before = PANICS.load(Ordering::SeqCst);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let finished = catch_unwind(AssertUnwindSafe(|| {
            let mut s = fresh();
            let _ = s.execute(id, params);
        }));
        let _ = tx.send(finished.is_ok());
    });
    rx.recv_timeout(DEADLINE).ok().map(|ok| ok && PANICS.load(Ordering::SeqCst) == before)
}

#[test]
#[ignore = "full-registry fuzz; run with --ignored"]
fn no_command_panics_or_hangs_on_adversarial_params() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        PANICS.fetch_add(1, Ordering::SeqCst);
        default_hook(info);
    }));
    let mut failures = Vec::new();
    for spec in command_specs() {
        for params in pistudiodashboard_testkit::adversarial_params() {
            let shown: String = params.to_string().chars().take(120).collect();
            match run_watched(spec.id, params) {
                Some(true) => {}
                Some(false) => failures.push(format!("{} panicked on {shown}", spec.id)),
                None => failures.push(format!("{} hung on {shown}", spec.id)),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
