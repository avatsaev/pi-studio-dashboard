//! Smoke against a real pi-studio daemon (see swe/notes/smoke-harness.md). Ignored by default:
//!
//! ```sh
//! PI_STUDIO_SMOKE=127.0.0.1:6790 cargo test -p pistudiodashboard-client --test live_daemon -- --ignored --nocapture
//! ```
//!
//! `PI_STUDIO_SMOKE_PASSWORD` sets a password; `PI_STUDIO_SMOKE_EXPECT=auth_failed` expects the
//! daemon to reject it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use pistudiodashboard_client::{ClientConfig, ClientHandle, DaemonEvent, DaemonEventKind, OutboundRequest, Secret, UiResponse};
use pistudiodashboard_model::ConnState;
use support::{closed_port, entry, is_state};

fn target() -> (String, u16) {
    let addr = std::env::var("PI_STUDIO_SMOKE").unwrap_or_else(|_| "127.0.0.1:6790".to_owned());
    let (host, port) = addr.rsplit_once(':').expect("host:port");
    (host.to_owned(), port.parse().expect("port"))
}

fn show(e: &DaemonEvent) -> String {
    format!("epoch={} seq={} server={:?} {:?}", e.conn_epoch, e.seq, e.server_id, e.kind)
}

/// Drain batches for `window`, printing every event; returns them all.
fn drain(handle_rx: &mut tokio::sync::mpsc::Receiver<Vec<DaemonEvent>>, rt: &tokio::runtime::Runtime, window: Duration) -> Vec<DaemonEvent> {
    rt.block_on(async {
        let mut all = Vec::new();
        let end = tokio::time::Instant::now() + window;
        while let Ok(Some(batch)) = tokio::time::timeout_at(end, handle_rx.recv()).await {
            println!("batch of {}", batch.len());
            for e in batch {
                println!("  {}", show(&e));
                all.push(e);
            }
        }
        all
    })
}

#[test]
#[ignore = "needs a running pi-studio daemon"]
fn live_daemon_connects_handshakes_and_disconnects() {
    let (host, port) = target();
    let (client, mut rx) = ClientHandle::start(ClientConfig::new("0.1.0"));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut e = entry("smoke", port);
    e.host = host;
    client.connect(e, None).unwrap();
    let events = drain(&mut rx, &rt, Duration::from_secs(3));
    assert!(events.iter().any(|e| is_state(e, ConnState::Live)), "never went live");
    assert!(events.iter().any(|e| matches!(&e.kind, DaemonEventKind::Hello { .. })));
    client.disconnect("smoke");
    let after = drain(&mut rx, &rt, Duration::from_secs(2));
    assert!(after.iter().any(|e| is_state(e, ConnState::Idle)));
    client.shutdown();
}

#[test]
#[ignore = "binds and releases a local port"]
fn closed_port_shows_backoff() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let port = rt.block_on(closed_port());
    let (client, mut rx) = ClientHandle::start(ClientConfig::new("0.1.0"));
    client.connect(entry("closed", port), None).unwrap();
    let events = drain(&mut rx, &rt, Duration::from_secs(6));
    assert!(events.iter().any(|e| is_state(e, ConnState::Backoff)));
    client.shutdown();
}

#[test]
#[ignore = "needs a running pi-studio daemon; takes ~19 s"]
fn live_daemon_rtt_and_diagnostics() {
    let (host, port) = target();
    let (client, mut rx) = ClientHandle::start(ClientConfig::new("0.1.0"));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut e = entry("smoke", port);
    e.host = host;
    let password = std::env::var("PI_STUDIO_SMOKE_PASSWORD").ok().map(Secret::new);
    client.connect(e, password).unwrap();
    let events = drain(&mut rx, &rt, Duration::from_secs(19));
    if std::env::var("PI_STUDIO_SMOKE_EXPECT").as_deref() == Ok("auth_failed") {
        assert!(events.iter().any(|e| is_state(e, ConnState::AuthFailed)));
        return;
    }
    assert!(events.iter().any(|e| matches!(e.kind, DaemonEventKind::Rtt { .. })), "no RTT within 19 s");
    assert!(events.iter().any(|e| matches!(e.kind, DaemonEventKind::Diagnostics(_))));
    client.shutdown();
}

/// Snapshot from the real `list_agents_response` / `agent_ui_list_response`, then the two
/// mutations against agents created for this purpose:
/// `PI_STUDIO_SMOKE_CONFIRM_AGENT` (a `#ui confirm` agent), `PI_STUDIO_SMOKE_INTERRUPT_AGENT`
/// (a running agent) and `PI_STUDIO_SMOKE_IDLE_AGENT` (an idle agent).
#[test]
#[ignore = "needs a running pi-studio daemon and agents created with its CLI"]
fn live_daemon_snapshot_and_mutations() {
    let (host, port) = target();
    let (client, mut rx) = ClientHandle::start(ClientConfig::new("0.1.0"));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut e = entry("smoke", port);
    e.host = host;
    client.connect(e, None).unwrap();
    let events = drain(&mut rx, &rt, Duration::from_secs(3));
    let (agents, pending) = events
        .iter()
        .find_map(|e| match &e.kind {
            DaemonEventKind::Snapshot { agents, ui_pending } => Some((agents.clone(), ui_pending.clone())),
            _ => None,
        })
        .expect("Snapshot");
    println!("SNAPSHOT: {} agents, {} pending dialogs", agents.len(), pending.len());
    for a in &agents {
        println!("  agent {} {} {:?}", a.agent_id, a.status, a.title);
    }
    for p in &pending {
        println!(
            "  pending {} agent={} method={} expects={} title={:?} options={:?}",
            p.request_id, p.agent_id, p.method, p.expects_response, p.title, p.options
        );
    }
    let env = |k: &str| std::env::var(k).ok();
    let mut expected = 0;
    if let Some(agent) = env("PI_STUDIO_SMOKE_CONFIRM_AGENT") {
        let dialog = pending.iter().find(|p| p.agent_id == agent && p.method == "confirm").expect("pending confirm dialog for the agent");
        for token in ["respond-1", "respond-2"] {
            let r = OutboundRequest::RespondUi { ui_request_id: dialog.request_id.clone(), response: UiResponse::Confirm { confirmed: true } };
            client.request("smoke", r, token).unwrap();
            expected += 1;
            let _ = drain(&mut rx, &rt, Duration::from_millis(700));
        }
    }
    for (var, token) in [("PI_STUDIO_SMOKE_INTERRUPT_AGENT", "interrupt-running"), ("PI_STUDIO_SMOKE_IDLE_AGENT", "interrupt-idle")] {
        if let Some(agent_id) = env(var) {
            client.request("smoke", OutboundRequest::InterruptAgent { agent_id }, token).unwrap();
            expected += 1;
            let _ = drain(&mut rx, &rt, Duration::from_millis(700));
        }
    }
    println!("sent {expected} mutations");
    client.shutdown();
}

/// Agent updates, stats and the drill-down against a real daemon, through the same path the app
/// uses: client events -> ingest reducers -> effects executed through the client.
///
/// `PI_STUDIO_SMOKE_CLI` is the pi-studio CLI prefix (e.g. `node …/cli.js -H 127.0.0.1:6790
/// --home /tmp/pi-studio-cli`); the test runs `run "$PI_STUDIO_SMOKE_PROMPT"` (default `hello`) with it
/// to create an agent live.
#[test]
#[ignore = "needs a running pi-studio daemon and PI_STUDIO_SMOKE_CLI"]
fn live_daemon_agent_lifecycle_stats_and_drilldown() {
    use pistudiodashboard_ingest::{apply_in_place, drill_select, stats_tick};
    use pistudiodashboard_model::{DaemonSlice, Effect, MonitorState};

    let (host, port) = target();
    let cli = std::env::var("PI_STUDIO_SMOKE_CLI").expect("PI_STUDIO_SMOKE_CLI");
    let (client, mut rx) = ClientHandle::start(ClientConfig::new("0.1.0"));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut e = entry("smoke", port);
    e.host = host;
    let mut state = MonitorState::default();
    state.daemons.push(DaemonSlice::new("smoke", "smoke"));
    client.connect(e, None).unwrap();

    let mut parts = cli.split_whitespace();
    let mut cmd = std::process::Command::new(parts.next().unwrap());
    let prompt = std::env::var("PI_STUDIO_SMOKE_PROMPT").unwrap_or_else(|_| "hello".to_owned());
    cmd.args(parts).args(["run", prompt.as_str(), "--cwd", "/tmp", "-p", "mock/mock"]);
    let mut child = None;
    let mut drilled: Option<String> = None;
    let started = std::time::Instant::now();
    let mut known: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut last_tick = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(14) {
        let batch = rt.block_on(async { tokio::time::timeout(Duration::from_millis(250), rx.recv()).await });
        let mut effects = Vec::new();
        if let Ok(Some(batch)) = batch {
            for ev in &batch {
                let before: Vec<_> = state.agents.values().map(|a| (a.agent_id.clone(), a.status)).collect();
                apply_in_place(&mut state, ev, 1, &mut effects);
                match &ev.kind {
                    DaemonEventKind::Snapshot { agents, .. } => {
                        println!("[{:>5}ms] Snapshot with {} agents", started.elapsed().as_millis(), agents.len());
                        known = agents.iter().map(|a| a.agent_id.clone()).collect();
                        child = Some(cmd.spawn().expect("spawn pi-studio CLI"));
                    }
                    DaemonEventKind::AgentUpdate { agent_id, status, title, .. } => {
                        println!("[{:>5}ms] AgentUpdate {agent_id} status={status:?} title={title:?}", started.elapsed().as_millis());
                        if !known.contains(agent_id) && drilled.is_none() {
                            drilled = Some(agent_id.clone());
                        }
                    }
                    DaemonEventKind::AgentStream { agent_id, seq, event, .. } if !known.contains(agent_id) => {
                        println!("[{:>5}ms] AgentStream {agent_id} seq={seq:?} {event:?}", started.elapsed().as_millis());
                    }
                    DaemonEventKind::Stats { agent_id, result } => println!("[{:>5}ms] Stats {agent_id} {result:?}", started.elapsed().as_millis()),
                    DaemonEventKind::Timeline { agent_id, items, reset, seq_end } => {
                        println!("[{:>5}ms] Timeline {agent_id} items={} reset={reset} seqEnd={seq_end}", started.elapsed().as_millis(), items.len());
                    }
                    _ => {}
                }
                if let DaemonEventKind::AgentStream { agent_id, .. } = &ev.kind
                    && let Some(a) = state.agents.values().find(|a| &a.agent_id == agent_id).filter(|_| !known.contains(agent_id))
                {
                    println!(
                        "          card {agent_id}: turn={:?} line={:?} tool={:?} err={:?} errored={}",
                        a.activity.turn_state,
                        a.activity.last_output_line,
                        a.activity.current_tool.as_ref().map(|t| &t.label),
                        a.activity.last_error_message,
                        a.attention.errored
                    );
                }
                let after: Vec<_> = state.agents.values().map(|a| (a.agent_id.clone(), a.status)).collect();
                if before != after {
                    for (id, st) in after.iter().filter(|a| !before.contains(a)) {
                        println!("          card {id} -> {st:?}");
                    }
                }
            }
        }
        if last_tick.elapsed() >= Duration::from_secs(2) {
            last_tick = std::time::Instant::now();
            effects.extend(stats_tick(&state, 0));
        }
        if let Some(id) = drilled.clone().filter(|_| started.elapsed() > Duration::from_secs(6) && state.drilldown.is_none()) {
            let sid = state.daemons[0].server_id.clone().unwrap_or_default();
            if state.agent(&sid, &id).is_some() {
                println!("[{:>5}ms] agent.select {id}", started.elapsed().as_millis());
                effects.extend(drill_select(&mut state, &sid, &id).unwrap());
            }
        }
        for eff in effects {
            match eff {
                Effect::FetchStats { server_id, agent_id } => {
                    let _ = client.request(&server_id, OutboundRequest::SessionStats { agent_id }, "");
                }
                Effect::FetchTimeline { server_id, agent_id } => {
                    let _ = client.request(&server_id, OutboundRequest::FetchTimeline { agent_id }, "");
                }
                _ => {}
            }
        }
    }
    if let Some(mut c) = child {
        let _ = c.wait();
    }
    let sid = state.daemons[0].server_id.clone().unwrap_or_default();
    let new_agent = drilled.expect("the CLI-created agent appeared");
    let card = state.agent(&sid, &new_agent).expect("card");
    println!(
        "FINAL card {new_agent}: status={:?} turn={:?} lastLine={:?} tool={:?}",
        card.status, card.activity.turn_state, card.activity.last_output_line, card.activity.current_tool
    );
    println!("FINAL usage {:?}", card.usage);
    if let Some(d) = &state.drilldown {
        println!("FINAL drilldown target={} state={:?} seqHigh={:?} items={}", d.target.agent_id, d.state, d.seq_high, d.items.len());
        for it in d.items.iter().take(12) {
            println!("   {it:?}");
        }
    }
    println!("{:?}", state.validate());
    client.shutdown();
}
