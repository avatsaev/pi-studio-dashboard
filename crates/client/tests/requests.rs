//! Outbound requests, correlation and the mutation allowlist against the scripted fake daemon.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use pistudiodashboard_client::{ClientConfig, ClientHandle, DaemonEvent, DaemonEventKind, MutationOp, MutationResult, OutboundRequest, SendError, UiResponse};
use pistudiodashboard_model::{ConnState, CountersSnapshot};
use pistudiodashboard_protocol::outbound::READ_TYPES;
use pistudiodashboard_protocol::{MUTATION_ALLOWLIST, all_variants};
use serde_json::{Value, json};
use support::fake_daemon::{Behavior, FakeDaemon, Handshake, Responder, inner_type};
use support::{collect_count, collect_until, entry, is_state};
use tokio::time::Instant;

fn start() -> (ClientHandle, support::Events) {
    let (client, rx) = ClientHandle::start_in(tokio::runtime::Handle::current(), ClientConfig::new("0.1.0"));
    (client, support::Events::new(rx))
}

async fn live(daemon: &FakeDaemon) -> (ClientHandle, support::Events) {
    let (client, mut rx) = start();
    client.connect(entry("e1", daemon.port()), None).unwrap();
    collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
    (client, rx)
}

fn session(message: Value) -> String {
    json!({"type": "session", "message": message}).to_string()
}

fn results(events: &[DaemonEvent]) -> Vec<(String, MutationOp, MutationResult)> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            DaemonEventKind::RequestResult { token, op, result } => Some((token.clone(), *op, result.clone())),
            _ => None,
        })
        .collect()
}

fn diagnostics(e: &DaemonEvent) -> Option<&CountersSnapshot> {
    match &e.kind {
        DaemonEventKind::Diagnostics(c) => Some(c),
        _ => None,
    }
}

/// Answers the two mutations according to the request's ids.
fn mutation_responder() -> Responder {
    Responder::new(|f| {
        let m = &f["message"];
        let rid = m["requestId"].clone();
        match inner_type(f) {
            "agent_ui_respond_request" => {
                let payload = match m["uiRequestId"].as_str() {
                    Some("ok") => json!({"ok": true}),
                    Some("gone") => json!({"ok": false, "error": "not_found"}),
                    Some("boom") => json!({"ok": false, "error": "boom"}),
                    _ => return vec![session(json!({"type": "rpc_error", "requestId": rid, "code": "handler_error", "message": "nope"}))],
                };
                vec![session(json!({"type": "agent_ui_respond_response", "requestId": rid, "payload": payload}))]
            }
            "interrupt_agent" => {
                let ok = m["agentId"] == "running";
                vec![session(json!({"type": "interrupt_response", "requestId": rid, "agentId": m["agentId"], "ok": ok}))]
            }
            _ => vec![],
        }
    })
}

#[tokio::test]
async fn respond_ui_results_and_tokens() {
    let daemon = FakeDaemon::start(Behavior { responder: Some(mutation_responder()), ..Behavior::default() }).await;
    let (client, mut rx) = live(&daemon).await;
    for (ui, token) in [("ok", "t-ok"), ("gone", "t-gone"), ("boom", "t-boom"), ("weird", "t-rpc")] {
        let r = OutboundRequest::RespondUi { ui_request_id: ui.into(), response: UiResponse::Confirm { confirmed: true } };
        client.request("e1", r, token).unwrap();
    }
    let mut got = results(&collect_count(&mut rx, 4, |e| matches!(e.kind, DaemonEventKind::RequestResult { .. })).await);
    got.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        got,
        [
            ("t-boom".to_owned(), MutationOp::RespondUi, MutationResult::Rejected { error: "boom".into() }),
            ("t-gone".to_owned(), MutationOp::RespondUi, MutationResult::NotFound),
            ("t-ok".to_owned(), MutationOp::RespondUi, MutationResult::Ok),
            ("t-rpc".to_owned(), MutationOp::RespondUi, MutationResult::RpcError { code: Some("handler_error".into()), message: Some("nope".into()) }),
        ]
    );
}

#[tokio::test]
async fn interrupt_result_carries_the_daemons_flag_and_requests_are_addressable_by_server_id() {
    let daemon = FakeDaemon::start(Behavior { responder: Some(mutation_responder()), ..Behavior::default() }).await;
    let (client, mut rx) = live(&daemon).await;
    client.request("e1", OutboundRequest::InterruptAgent { agent_id: "idle".into() }, "t1").unwrap();
    // Addressed by the serverId learned from Hello.
    client.request("00000000-0000-4000-8000-0000000000aa", OutboundRequest::InterruptAgent { agent_id: "running".into() }, "t2").unwrap();
    let mut got = results(&collect_count(&mut rx, 2, |e| matches!(e.kind, DaemonEventKind::RequestResult { .. })).await);
    got.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(got[0], ("t1".to_owned(), MutationOp::InterruptAgent, MutationResult::Interrupt { ok: false }));
    assert_eq!(got[1], ("t2".to_owned(), MutationOp::InterruptAgent, MutationResult::Interrupt { ok: true }));
}

#[tokio::test(start_paused = true)]
async fn unanswered_mutation_times_out_and_unanswered_read_counts_a_late_response() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = live(&daemon).await;
    client.request("e1", OutboundRequest::ListAgents, "").unwrap();
    client.request("e1", OutboundRequest::InterruptAgent { agent_id: "a".into() }, "t-int").unwrap();
    let start = Instant::now();
    let events = collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::RequestResult { .. })).await;
    assert_eq!(results(&events), [("t-int".to_owned(), MutationOp::InterruptAgent, MutationResult::Timeout)]);
    assert!(start.elapsed() >= Duration::from_secs(15));

    // The read timed out at 10 s; its answer, arriving now, is late.
    let list = daemon.received().into_iter().rfind(|f| inner_type(f) == "list_agents_request").unwrap();
    daemon.push(session(json!({"type": "list_agents_response", "requestId": list["message"]["requestId"], "agents": []})));
    let events = collect_until(&mut rx, |e| diagnostics(e).is_some_and(|c| c.late_responses == 1)).await;
    assert!(events.iter().all(|e| !matches!(e.kind, DaemonEventKind::RequestResult { .. })));
}

#[tokio::test]
async fn pending_mutation_is_lost_with_the_connection_and_never_replayed() {
    let daemon = FakeDaemon::start(Behavior { close_on: Some("interrupt_agent".into()), ..Behavior::default() }).await;
    let (client, mut rx) = live(&daemon).await;
    client.request("e1", OutboundRequest::InterruptAgent { agent_id: "a".into() }, "t-lost").unwrap();
    let events = collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::RequestResult { .. })).await;
    assert_eq!(results(&events), [("t-lost".to_owned(), MutationOp::InterruptAgent, MutationResult::ConnectionLost)]);
    assert!(events.iter().all(|e| !is_state(e, ConnState::Live)));
    // Reconnects; the mutation is not sent again.
    collect_until(&mut rx, |e| e.conn_epoch >= 2 && is_state(e, ConnState::Live)).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let sent = daemon.received().iter().filter(|f| inner_type(f) == "interrupt_agent").count();
    assert_eq!(sent, 1);
}

#[tokio::test]
async fn requests_while_not_live_fail_at_once_and_queue_nothing() {
    let daemon = FakeDaemon::start(Behavior { handshake: Handshake::Silent, ..Behavior::default() }).await;
    let (client, mut rx) = start();
    let respond = || OutboundRequest::RespondUi { ui_request_id: "u".into(), response: UiResponse::Select { value: "a".into() } };
    assert_eq!(client.request("e1", respond(), "t"), Err(SendError::NotConnected), "unknown entry");
    client.connect(entry("e1", daemon.port()), None).unwrap();
    collect_until(&mut rx, |e| is_state(e, ConnState::Handshaking)).await;
    assert_eq!(client.request("e1", respond(), "t"), Err(SendError::NotConnected), "handshaking");
    assert_eq!(client.request("e1", OutboundRequest::ListAgents, ""), Err(SendError::NotConnected));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(daemon.received().iter().all(|f| inner_type(f) == "hello"));
    // Not replayed once the connection later becomes live (only the sync requests go out).
    daemon.set(|b| b.handshake = Handshake::ServerInfo { server_id: Some("s".into()), features: None });
    client.restart("e1", entry("e1", daemon.port()), None).unwrap();
    collect_until(&mut rx, |e| is_state(e, ConnState::Live)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let types: Vec<String> = daemon.received().iter().map(|f| inner_type(f).to_owned()).collect();
    assert!(types.iter().all(|t| matches!(t.as_str(), "hello" | "list_agents_request" | "agent_ui_list_request")), "{types:?}");
}

#[tokio::test]
async fn reads_beyond_256_outstanding_fail_fast_with_busy() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, _rx) = live(&daemon).await;
    let outcomes: Vec<_> = (0..300).map(|_| client.request("e1", OutboundRequest::ListAgents, "")).collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 256);
    assert!(outcomes[256..].iter().all(|r| *r == Err(SendError::Busy)));
    // A mutation is still accepted at the limit.
    assert!(client.request("e1", OutboundRequest::InterruptAgent { agent_id: "a".into() }, "t").is_ok());
    tokio::time::sleep(Duration::from_millis(500)).await;
    let reads = daemon.received().iter().filter(|f| inner_type(f) == "list_agents_request").count();
    assert_eq!(reads, 257, "the sync request plus 256 accepted reads");
}

#[tokio::test]
async fn every_variant_sent_stays_inside_the_allowlist() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, _rx) = live(&daemon).await;
    for r in all_variants() {
        client.request("e1", r, "t").unwrap();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let types: Vec<String> = daemon.received().iter().map(|f| inner_type(f).to_owned()).collect();
    let allowed = |t: &str| t == "hello" || READ_TYPES.contains(&t) || MUTATION_ALLOWLIST.contains(&t);
    assert!(types.iter().all(|t| allowed(t)), "{types:?}");
    for want in READ_TYPES.iter().chain(MUTATION_ALLOWLIST.iter()) {
        assert!(types.iter().any(|t| t == want), "{want} was never sent: {types:?}");
    }
    let mutations = types.iter().filter(|t| MUTATION_ALLOWLIST.contains(&t.as_str())).count();
    assert_eq!(mutations, 3, "two RespondUi and one InterruptAgent");
}
