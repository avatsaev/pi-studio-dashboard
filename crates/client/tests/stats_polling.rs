//! The D16/D39 stats schedule end to end: reducers (ingest) decide, a recording owner loop
//! executes effects through the client, the fake daemon answers. The owner loop's tick period
//! is shortened from 10 s to 200 ms; everything else is the production code path.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use pistudiodashboard_client::{ClientConfig, ClientHandle, OutboundRequest};
use pistudiodashboard_ingest::{apply_in_place, stats_tick};
use pistudiodashboard_model::{DaemonSlice, Effect, MonitorState, UsageSnapshot};
use serde_json::{Value, json};
use support::entry;
use support::fake_daemon::{AgentsReply, Behavior, FakeDaemon, Responder, inner_type};

fn session(m: Value) -> String {
    json!({"type": "session", "message": m}).to_string()
}

fn stats_for(daemon: &FakeDaemon, agent: &str) -> usize {
    daemon.received().iter().filter(|f| inner_type(f) == "agent_session_stats_request" && f["message"]["agentId"] == agent).count()
}

fn execute(client: &ClientHandle, effects: Vec<Effect>) {
    for e in effects {
        if let Effect::FetchStats { server_id, agent_id } = e {
            let _ = client.request(&server_id, OutboundRequest::SessionStats { agent_id }, "");
        }
    }
}

#[tokio::test]
async fn running_agents_are_polled_periodically_idle_ones_once_and_a_finished_turn_refreshes() {
    let responder = Responder::new(|f| {
        if inner_type(f) != "agent_session_stats_request" {
            return vec![];
        }
        vec![session(json!({"type": "agent_session_stats_response", "requestId": f["message"]["requestId"], "payload": {"tokens": {"total": 12}}}))]
    });
    let agents = vec![json!({"agentId": "run", "status": "running"}), json!({"agentId": "idle", "status": "idle"})];
    let daemon = FakeDaemon::start(Behavior { agents: AgentsReply::Agents(agents), responder: Some(responder), ..Behavior::default() }).await;

    let (client, rx) = ClientHandle::start_in(tokio::runtime::Handle::current(), ClientConfig::new("0.1.0"));
    let mut rx = support::Events::new(rx);
    let mut state = MonitorState::default();
    state.daemons.push(DaemonSlice::new("e1", "box"));
    client.connect(entry("e1", daemon.port()), None).unwrap();

    let mut tick = tokio::time::interval(Duration::from_millis(200));
    let started = tokio::time::Instant::now();
    let mut completed_at = None;
    let mut before_completion = 0;
    while started.elapsed() < Duration::from_millis(2600) {
        tokio::select! {
            Some(batch) = rx.recv() => {
                let mut effects = Vec::new();
                for e in &batch {
                    apply_in_place(&mut state, e, 1, &mut effects);
                }
                execute(&client, effects);
            }
            _ = tick.tick() => {
                let effects = stats_tick(&state, 0);
                execute(&client, effects);
                if completed_at.is_none() && started.elapsed() > Duration::from_millis(1200) {
                    before_completion = stats_for(&daemon, "run");
                    completed_at = Some(started.elapsed());
                    daemon.push(session(json!({"type": "agent_stream", "agentId": "run", "seq": 1, "event": {"kind": "turn_completed"}})));
                    daemon.push(session(json!({"type": "agent_update", "agentId": "run", "status": "idle"})));
                }
            }
        }
    }
    assert!(completed_at.is_some());
    assert!(before_completion >= 3, "one after the snapshot plus >= 2 periodic requests while running, got {before_completion}");
    let after = stats_for(&daemon, "run");
    assert!(after > before_completion, "turn_completed triggers a refresh ({before_completion} -> {after})");
    assert!(after <= before_completion + 2, "and no periodic polling after the agent went idle ({before_completion} -> {after})");
    assert_eq!(stats_for(&daemon, "idle"), 1, "an idle agent is asked once, after the snapshot");
    let run = state.agent("00000000-0000-4000-8000-0000000000aa", "run");
    assert!(matches!(run.map(|a| &a.usage), Some(UsageSnapshot::Some { tokens: Some(t), .. }) if t.total == Some(12)));
}
