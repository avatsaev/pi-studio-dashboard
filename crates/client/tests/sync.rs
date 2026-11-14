//! Sync-on-connect: requests, `Snapshot`, buffering of racing pushes and failure handling.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use pistudiodashboard_client::{ClientConfig, ClientHandle, DaemonEvent, DaemonEventKind};
use pistudiodashboard_model::{ConnState, CountersSnapshot};
use serde_json::{Value, json};
use support::fake_daemon::{AgentsReply, Behavior, FakeDaemon, UiReply, inner_type};
use support::{collect_until, entry, is_state};
use tokio::time::Instant;

fn start() -> (ClientHandle, support::Events) {
    let (client, rx) = ClientHandle::start_in(tokio::runtime::Handle::current(), ClientConfig::new("0.1.0"));
    (client, support::Events::new(rx))
}

fn is_snapshot(e: &DaemonEvent) -> bool {
    matches!(e.kind, DaemonEventKind::Snapshot { .. })
}

fn snapshot(events: &[DaemonEvent]) -> (&Vec<pistudiodashboard_model::AgentListItem>, &Vec<pistudiodashboard_model::PendingUi>) {
    events
        .iter()
        .find_map(|e| match &e.kind {
            DaemonEventKind::Snapshot { agents, ui_pending } => Some((agents, ui_pending)),
            _ => None,
        })
        .expect("a Snapshot event")
}

fn live_error_code(events: &[DaemonEvent]) -> Option<String> {
    events.iter().find_map(|e| match &e.kind {
        DaemonEventKind::ConnState { state: ConnState::Live, last_error, .. } => last_error.as_ref().map(|l| l.code.clone()),
        _ => None,
    })
}

fn agent(id: &str, status: &str) -> Value {
    json!({"agentId": id, "status": status, "title": format!("title {id}"), "cwd": "/tmp", "labels": {"team": "x"}, "lastActivity": 1_767_225_600_000_i64, "provider": "mock"})
}

fn dialog(id: &str, method: &str, expects: bool, removed: bool) -> Value {
    json!({"requestId": id, "agentId": "a1", "method": method, "expectsResponse": expects, "removed": removed,
           "payload": {"title": "Pick", "message": "which?", "options": ["a", "b"]}, "createdAt": 1_767_225_600_000_i64, "timeoutMs": 5000})
}

fn diagnostics(e: &DaemonEvent) -> Option<&CountersSnapshot> {
    match &e.kind {
        DaemonEventKind::Diagnostics(c) => Some(c),
        _ => None,
    }
}

#[tokio::test]
async fn snapshot_is_built_from_both_lists_after_exactly_two_requests() {
    let daemon = FakeDaemon::start(Behavior {
        agents: AgentsReply::Agents(vec![agent("a1", "running"), json!({"agentId": "a2", "status": "hyperspace", "title": 42}), json!({"status": "idle"})]),
        ui: UiReply::Pending(vec![
            dialog("d1", "select", true, false),
            dialog("d2", "setStatus", false, false),
            dialog("d3", "confirm", true, true),
            json!({"agentId": "a1"}),
        ]),
        ..Behavior::default()
    })
    .await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let events = collect_until(&mut rx, is_snapshot).await;

    let shape: Vec<String> = events.iter().filter_map(|e| support::state_of(e).map(|s| format!("{s:?}"))).collect();
    assert_eq!(shape, ["Probing", "Connecting", "Handshaking", "Syncing", "Live"]);
    assert!(is_snapshot(events.last().unwrap()), "Snapshot comes right after Live");
    assert!(live_error_code(&events).is_none());

    let (agents, pending) = snapshot(&events);
    assert_eq!(agents.len(), 2, "the agent without an id is dropped");
    assert_eq!((agents[0].agent_id.as_str(), agents[0].status.as_str(), agents[0].title.as_deref()), ("a1", "running", Some("title a1")));
    assert_eq!(agents[0].labels, ["team=x"]);
    assert_eq!((agents[1].status.as_str(), agents[1].title.clone()), ("unknown", None));
    assert_eq!(pending.len(), 1, "no non-responding, removed or malformed dialogs");
    let d = &pending[0];
    assert_eq!((d.request_id.as_str(), d.method.as_str(), d.expects_response, d.timeout_ms), ("d1", "select", true, Some(5000)));
    assert_eq!((d.title.as_deref(), d.options.clone()), (Some("Pick"), Some(vec!["a".to_owned(), "b".to_owned()])));

    let sent: Vec<String> = daemon.received().iter().map(|f| inner_type(f).to_owned()).collect();
    assert_eq!(sent, ["hello", "list_agents_request", "agent_ui_list_request"]);

    let diag = collect_until(&mut rx, |e| diagnostics(e).is_some()).await;
    let c = diagnostics(diag.last().unwrap()).unwrap();
    assert_eq!((c.unknown_enum_values, c.malformed), (1, 2), "one unknown status; one agent and one dialog dropped");
}

#[tokio::test]
async fn a_push_that_raced_the_list_is_emitted_after_the_snapshot() {
    let resolved = json!({"type": "session", "message": {"type": "agent_ui_resolved", "requestId": "d1", "agentId": "a1", "reason": "answered"}}).to_string();
    let daemon = FakeDaemon::start(Behavior {
        ui: UiReply::Pending(vec![dialog("d1", "confirm", true, false)]),
        push_before_ui_list: vec![resolved],
        ..Behavior::default()
    })
    .await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let events = collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::UiResolved { .. })).await;
    let snapshot_at = events.iter().position(is_snapshot).expect("snapshot before the push");
    let resolved_at = events.len() - 1;
    assert!(snapshot_at < resolved_at);
    let DaemonEventKind::UiResolved { request_id, agent_id, reason } = &events[resolved_at].kind else { panic!() };
    assert_eq!((request_id.as_str(), agent_id.as_str(), reason.as_str()), ("d1", "a1", "answered"));
    // The stale list still names the dialog; the reducer removes it because the push comes after.
    assert_eq!(snapshot(&events).1.len(), 1);
    assert!(events.windows(2).all(|w| w[0].seq < w[1].seq));
}

#[tokio::test(start_paused = true)]
async fn ui_list_failures_leave_the_connection_live_with_an_empty_list_and_a_warning() {
    for ui in [UiReply::Silent, UiReply::Error, UiReply::NotOk] {
        let silent = matches!(ui, UiReply::Silent);
        let daemon = FakeDaemon::start(Behavior { agents: AgentsReply::Agents(vec![agent("a1", "idle")]), ui, ..Behavior::default() }).await;
        let (client, mut rx) = start();
        let started = Instant::now();
        client.connect(entry("e1", daemon.port()), None).unwrap();
        let events = collect_until(&mut rx, is_snapshot).await;
        let (agents, pending) = snapshot(&events);
        assert_eq!((agents.len(), pending.len()), (1, 0));
        assert!(events.iter().any(|e| is_state(e, ConnState::Live)));
        assert_eq!(live_error_code(&events).as_deref(), Some("timeout"), "warning recorded on the live state");
        if silent {
            assert!(started.elapsed() >= Duration::from_secs(10), "waited for the 10 s timeout");
        }
        client.remove("e1");
    }
}

#[tokio::test(start_paused = true)]
async fn list_agents_failure_backs_off() {
    for (agents, code) in [(AgentsReply::Silent, "timeout"), (AgentsReply::Error, "bad_handshake")] {
        let daemon = FakeDaemon::start(Behavior { agents, ..Behavior::default() }).await;
        let (client, mut rx) = start();
        client.connect(entry("e1", daemon.port()), None).unwrap();
        let events = collect_until(&mut rx, |e| is_state(e, ConnState::Backoff)).await;
        assert!(!events.iter().any(|e| is_state(e, ConnState::Live) || is_snapshot(e)));
        let last = events.last().unwrap();
        let DaemonEventKind::ConnState { last_error: Some(err), next_retry_in_ms, .. } = &last.kind else { panic!() };
        assert_eq!(err.code, code);
        assert!(next_retry_in_ms.is_some());
        client.remove("e1");
    }
}

#[tokio::test]
async fn reconnect_resyncs_and_the_new_snapshot_carries_the_next_epoch() {
    let daemon = FakeDaemon::start(Behavior { agents: AgentsReply::Agents(vec![agent("a1", "running")]), ..Behavior::default() }).await;
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let first = collect_until(&mut rx, is_snapshot).await;
    daemon.set(|b| b.agents = AgentsReply::Agents(vec![agent("a1", "idle"), agent("a3", "error")]));
    client.restart("e1", entry("e1", daemon.port()), None).unwrap();
    let second = collect_until(&mut rx, is_snapshot).await;
    let (agents, _) = snapshot(&second);
    assert_eq!(agents.iter().map(|a| (a.agent_id.as_str(), a.status.as_str())).collect::<Vec<_>>(), [("a1", "idle"), ("a3", "error")]);
    assert_eq!(second.last().unwrap().conn_epoch, first.last().unwrap().conn_epoch + 1);
    assert_eq!(second.last().unwrap().seq, 8, "seq restarts per attempt: Probing, Health, Connecting, Handshaking, Hello, Syncing, Live, Snapshot");
}
