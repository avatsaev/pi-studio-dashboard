//! `list_agents_response` / `agent_ui_list_response` against the vendored fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pistudiodashboard_protocol::{AgentStatus, ListAgentsResponse, UiListResponse, UiResolved};
use serde_json::{Value, json};

fn message(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pi-studio").join(name);
    serde_json::from_str::<Value>(&std::fs::read_to_string(p).unwrap()).unwrap()["frame"]["message"].clone()
}

#[test]
fn list_agents_fixture() {
    let r = ListAgentsResponse::from_message(&message("list_agents_response.json")).unwrap();
    assert_eq!((r.request_id.as_str(), r.malformed, r.unknown_status), ("req-1", 0, 0));
    let a = &r.agents[0];
    assert_eq!(a.agent_id, "00000000-0000-4000-8000-000000000001");
    assert_eq!(a.status, AgentStatus::Idle);
    assert_eq!((a.title.as_deref(), a.cwd.as_deref()), (Some("hello"), Some("/tmp")));
    assert_eq!((a.provider.as_deref(), a.model.as_deref(), a.thinking_level.as_deref()), (Some("mock"), Some("mock-model"), Some("off")));
    assert_eq!((a.model_provider.clone(), a.last_activity), (None, Some(1_767_225_600_000)));
    assert!(a.labels.is_empty());
}

#[test]
fn ui_list_fixture_keeps_the_pending_select_dialog() {
    let r = UiListResponse::from_message(&message("agent_ui_list_response.json")).unwrap();
    assert!(r.ok && r.malformed == 0 && r.error.is_none());
    assert_eq!(r.pending.len(), 1, "surfaces are not attention items");
    let d = &r.pending[0];
    assert_eq!((d.method.as_str(), d.expects_response, d.removed, d.truncated), ("select", true, false, false));
    assert_eq!(d.title(), Some("Pick one"));
    assert_eq!(d.options(), Some(vec!["a".to_owned(), "b".to_owned()]));
    assert_eq!(d.created_at, 1_767_225_600_000);
    assert!(d.is_attention());
}

#[test]
fn removed_and_non_responding_entries_are_not_attention() {
    let m = json!({"requestId":"r","payload":{"ok":true,"pending":[
        {"requestId":"1","agentId":"a","method":"setStatus","expectsResponse":false,"createdAt":1},
        {"requestId":"2","agentId":"a","method":"select","expectsResponse":true,"removed":true,"createdAt":1},
        {"requestId":"3","agentId":"a","method":"confirm","expectsResponse":true,"createdAt":1},
        {"agentId":"a","method":"confirm"}
    ]}});
    let r = UiListResponse::from_message(&m).unwrap();
    assert_eq!((r.pending.len(), r.malformed), (3, 1));
    let attention: Vec<_> = r.pending.iter().filter(|p| p.is_attention()).map(|p| p.request_id.as_str()).collect();
    assert_eq!(attention, ["3"]);
    let failed = UiListResponse::from_message(&json!({"requestId":"r","payload":{"ok":false,"error":"nope"}})).unwrap();
    assert!(!failed.ok && failed.error.as_deref() == Some("nope"));
    assert!(UiListResponse::from_message(&json!({"requestId":"r","payload":{"pending":[]}})).is_none());
}

#[test]
fn ui_resolved_push() {
    let m = message("agent_ui_resolved.answered.json");
    let r = UiResolved::from_message(&m).unwrap();
    assert_eq!(r.reason, "answered");
    assert!(UiResolved::from_message(&json!({"requestId":"r","agentId":"a"})).is_none());
}
