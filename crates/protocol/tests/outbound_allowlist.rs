//! D2/D36/D37: only two wire types can mutate, nothing else is expressible, and the encoded
//! frames match the vendored fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pistudiodashboard_protocol::outbound::READ_TYPES;
use pistudiodashboard_protocol::{InterruptReply, MUTATION_ALLOWLIST, OutboundRequest, RequestClass, RespondUiReply, UiResponse, all_variants, encode};
use serde_json::{Value, json};

fn fixture_frame(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pi-studio").join(name);
    serde_json::from_str::<Value>(&std::fs::read_to_string(p).unwrap()).unwrap()["frame"].clone()
}

fn decoded(r: &OutboundRequest) -> Value {
    serde_json::from_str(&encode(r, "req-1")).unwrap()
}

/// The `type` carried on the wire (inside the session envelope except for `ping`).
fn wire_type_of(v: &Value) -> &str {
    if v["type"] == "session" { v["message"]["type"].as_str().unwrap() } else { v["type"].as_str().unwrap() }
}

#[test]
fn only_the_two_allowlisted_types_are_mutations() {
    assert_eq!(MUTATION_ALLOWLIST, ["agent_ui_respond_request", "interrupt_agent"]);
    for r in all_variants() {
        let wire = decoded(&r);
        assert_eq!(wire_type_of(&wire), r.wire_type());
        let allowed = READ_TYPES.contains(&r.wire_type()) || MUTATION_ALLOWLIST.contains(&r.wire_type());
        assert!(allowed, "{} is neither read nor allowlisted", r.wire_type());
        let is_mutation = MUTATION_ALLOWLIST.contains(&r.wire_type());
        assert_eq!(r.class() == RequestClass::Mutation, is_mutation, "{}", r.wire_type());
        assert_eq!(wire["type"] == "session", r.wire_type() != "ping");
    }
    let mutations: Vec<_> = all_variants().into_iter().filter(|r| r.class() == RequestClass::Mutation).map(|r| r.wire_type()).collect();
    assert!(mutations.iter().all(|t| MUTATION_ALLOWLIST.contains(t)));
    assert!(MUTATION_ALLOWLIST.iter().all(|t| mutations.contains(t)));
}

/// Quoted string literals of the non-test part of a source file (everything before `#[cfg(test)]`).
fn literals(src: &str) -> Vec<String> {
    let body = src.split("#[cfg(test)]").next().unwrap_or(src);
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else { break };
        out.push(after[..end].to_owned());
        rest = &after[end + 1..];
    }
    out
}

#[test]
fn no_other_mutating_type_literal_and_no_permission_type_in_the_protocol_source() {
    const FORBIDDEN: [&str; 6] = ["delete_agent", "archive_agent", "send_agent_prompt", "kill_terminal", "create_agent_request", "subscribe_terminal_request"];
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut scanned = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "rs") {
            scanned += 1;
            for lit in literals(&std::fs::read_to_string(&path).unwrap()) {
                assert!(!FORBIDDEN.contains(&lit.as_str()), "{} mentions mutating type {lit}", path.display());
                assert!(!lit.to_lowercase().contains("permission"), "{} has a permission literal {lit}", path.display());
            }
        }
    }
    assert!(scanned >= 5);
    for r in all_variants() {
        assert!(!r.wire_type().contains("permission"));
    }
}

#[test]
fn respond_ui_matches_the_fixtures_and_has_no_extra_keys() {
    let confirm = decoded(&OutboundRequest::RespondUi { ui_request_id: "ui-req-1".into(), response: UiResponse::Confirm { confirmed: true } });
    assert_eq!(confirm, fixture_frame("agent_ui_respond_request.confirm.json"));
    let select = decoded(&OutboundRequest::RespondUi { ui_request_id: "ui-req-1".into(), response: UiResponse::Select { value: "a".into() } });
    assert_eq!(select, fixture_frame("agent_ui_respond_request.select.json"));
    assert_eq!(confirm["message"]["response"].as_object().unwrap().keys().collect::<Vec<_>>(), ["confirmed"]);
    assert_eq!(select["message"]["response"].as_object().unwrap().keys().collect::<Vec<_>>(), ["value"]);
}

#[test]
fn other_requests_match_their_fixtures() {
    assert_eq!(decoded(&OutboundRequest::ListAgents), fixture_frame("list_agents_request.json"));
    assert_eq!(decoded(&OutboundRequest::ListUi { agent_id: Some("agent-1".into()) }), fixture_frame("agent_ui_list_request.json"));
    assert_eq!(decoded(&OutboundRequest::SessionStats { agent_id: "agent-1".into() }), fixture_frame("agent_session_stats_request.json"));
    assert_eq!(decoded(&OutboundRequest::InterruptAgent { agent_id: "agent-1".into() }), fixture_frame("interrupt_agent.json"));
    assert_eq!(decoded(&OutboundRequest::Ping { client_sent_at: 1_767_225_600_000 }), {
        let mut f = fixture_frame("ping.json");
        f["requestId"] = json!("req-1");
        f
    });
    // The timeline fixture is the `after` page with limit 100: same keys as an explicit page.
    let page = decoded(&OutboundRequest::FetchTimelinePage {
        agent_id: "agent-1".into(),
        cursor: None,
        direction: pistudiodashboard_protocol::TimelineDirection::After,
        limit: 100,
    });
    assert_eq!(page, fixture_frame("fetch_agent_timeline_request.json"));
    let with_cursor = decoded(&OutboundRequest::FetchTimelinePage {
        agent_id: "a".into(),
        cursor: Some("27".into()),
        direction: pistudiodashboard_protocol::TimelineDirection::After,
        limit: 200,
    });
    assert_eq!(with_cursor["message"]["cursor"], "27");
}

#[test]
fn replies_decode_from_the_fixtures() {
    let ok = RespondUiReply::from_message(&fixture_frame("agent_ui_respond_response.ok.json")["message"]).unwrap();
    assert_eq!((ok.request_id.as_str(), ok.ok, ok.error), ("req-1", true, None));
    let nf = RespondUiReply::from_message(&fixture_frame("agent_ui_respond_response.not_found.json")["message"]).unwrap();
    assert_eq!((nf.ok, nf.error.as_deref()), (false, Some("not_found")));
    let boom = RespondUiReply::from_message(&json!({"requestId":"r","payload":{"ok":false,"error":"boom","x":1}})).unwrap();
    assert_eq!(boom.error.as_deref(), Some("boom"));
    assert!(RespondUiReply::from_message(&json!({"payload":{"ok":true}})).is_none());
    assert!(RespondUiReply::from_message(&json!({"requestId":"r","payload":{"ok":"yes"}})).is_none());

    let i = InterruptReply::from_message(&fixture_frame("interrupt_response.json")["message"]).unwrap();
    assert!(i.ok && i.request_id == "req-1" && i.agent_id.is_some());
    let off = InterruptReply::from_message(&json!({"type":"interrupt_response","requestId":"r","ok":false})).unwrap();
    assert!(!off.ok && off.agent_id.is_none());
    assert!(InterruptReply::from_message(&json!({"requestId":"r"})).is_none());
}
