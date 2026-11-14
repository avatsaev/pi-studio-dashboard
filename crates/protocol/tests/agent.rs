//! Agent pushes, stream events, stats and timeline pages against the vendored fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pistudiodashboard_protocol::{
    AgentArchived, AgentDeleted, AgentStatus, AgentStatusMsg, AgentStream, AgentUpdate, StatsResponse, StreamEvent, TimelinePage, TimelineReset,
};
use serde_json::{Value, json};

fn message(dir: &str, name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(dir).join(name);
    serde_json::from_str::<Value>(&std::fs::read_to_string(p).unwrap()).unwrap()["frame"]["message"].clone()
}

fn pi(name: &str) -> Value {
    message("pi-studio", name)
}

fn stream(name: &str) -> AgentStream {
    AgentStream::from_message(&pi(name)).unwrap_or_else(|| panic!("{name} did not decode"))
}

#[test]
fn every_stream_fixture_decodes_to_its_variant_and_only_partials_lack_a_seq() {
    type Check = fn(&StreamEvent) -> bool;
    let cases: [(&str, Check); 11] = [
        ("agent_stream.user_message.json", |e| matches!(e, StreamEvent::UserMessage { text: Some(t) } if t == "hello")),
        ("agent_stream.assistant_message.json", |e| matches!(e, StreamEvent::AssistantMessage { text, is_final: true } if text == "echo: hello")),
        ("agent_stream.reasoning.json", |e| matches!(e, StreamEvent::Reasoning { text: Some(t), is_final: false } if t == "mock reasoning")),
        (
            "agent_stream.tool_call.json",
            |e| matches!(e, StreamEvent::ToolCall { status, partial: false, tool, .. } if status == "running" && tool.kind == "shell" && tool.detail.as_deref() == Some("npm test")),
        ),
        (
            "agent_stream.tool_call.partial.json",
            |e| matches!(e, StreamEvent::ToolCall { partial: true, tool, .. } if tool.output.as_deref() == Some("> running suite A")),
        ),
        ("agent_stream.turn_started.json", |e| matches!(e, StreamEvent::TurnStarted)),
        ("agent_stream.turn_completed.json", |e| matches!(e, StreamEvent::TurnCompleted)),
        ("agent_stream.turn_failed.json", |e| matches!(e, StreamEvent::TurnFailed { error: Some(m) } if m == "mock turn failure")),
        ("agent_stream.turn_canceled.json", |e| matches!(e, StreamEvent::TurnCanceled)),
        ("agent_stream.error.json", |e| matches!(e, StreamEvent::Error { message: Some(m) } if m == "mock stream error")),
        ("agent_stream.compaction.json", |e| matches!(e, StreamEvent::Compaction { phase: Some(p), reason: Some(r) } if p == "started" && r == "manual")),
    ];
    for (name, check) in cases {
        let s = stream(name);
        assert!(check(&s.event), "{name}: {:?}", s.event);
        assert_eq!(s.seq.is_none(), name.contains("partial"), "{name}");
        assert_eq!(s.timestamp, Some(1_767_225_600_000), "{name}");
        assert_eq!(s.agent_id, "00000000-0000-4000-8000-000000000001");
    }
}

#[test]
fn foldable_events_are_exactly_deltas_and_partials() {
    assert!(stream("agent_stream.tool_call.partial.json").event.is_foldable());
    assert!(stream("agent_stream.reasoning.json").event.is_foldable());
    for name in ["agent_stream.tool_call.json", "agent_stream.turn_started.json", "agent_stream.turn_failed.json", "agent_stream.assistant_message.json"] {
        assert!(!stream(name).event.is_foldable(), "{name}");
    }
    assert!(StreamEvent::AssistantMessage { text: "x".into(), is_final: false }.is_foldable());
}

#[test]
fn unknown_and_malformed_stream_events() {
    let unknown = AgentStream::from_message(&message("local", "agent_stream.unknown_kind.json")).unwrap();
    assert_eq!(unknown.event, StreamEvent::Unknown { kind: "hologram_projected".into() });
    assert_eq!(unknown.seq, Some(1));
    let bad = |m: Value| AgentStream::from_message(&m);
    assert!(bad(json!({"agentId":"a","event":{"kind":"tool_call","status":"running"}})).is_none(), "tool_call needs a callId");
    assert!(bad(json!({"agentId":"a","event":{"text":"no kind"}})).is_none());
    assert!(bad(json!({"event":{"kind":"turn_started"}})).is_none());
    // Wrong-typed optionals become defaults, never errors.
    let lenient = bad(json!({"agentId":"a","seq":"x","timestamp":false,"event":{"kind":"assistant_message","text":7,"final":"yes"}})).unwrap();
    assert_eq!((lenient.seq, lenient.timestamp), (None, None));
    assert_eq!(lenient.event, StreamEvent::AssistantMessage { text: String::new(), is_final: false });
}

#[test]
fn long_texts_are_capped_on_char_boundaries() {
    let long = "é".repeat(100_000);
    let s = AgentStream::from_message(&json!({"agentId":"a","event":{"kind":"assistant_message","text":long}})).unwrap();
    let StreamEvent::AssistantMessage { text, .. } = s.event else { panic!() };
    assert!(text.len() <= 64 * 1024 && text.chars().all(|c| c == 'é'));
    let e = AgentStream::from_message(&json!({"agentId":"a","event":{"kind":"error","message":"x".repeat(5000)}})).unwrap();
    assert!(matches!(e.event, StreamEvent::Error { message: Some(m) } if m.len() == 1024));
}

#[test]
fn stream_events_round_trip_through_their_internal_json() {
    for name in ["agent_stream.tool_call.json", "agent_stream.assistant_message.json", "agent_stream.compaction.json", "agent_stream.turn_started.json"] {
        let e = stream(name).event;
        let back: StreamEvent = serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap();
        assert_eq!(back, e, "{name}");
    }
}

#[test]
fn agent_update_distinguishes_absent_from_null() {
    let a = AgentUpdate::from_message(&pi("agent_update.json")).unwrap();
    assert_eq!((a.status.as_deref(), a.title.clone(), a.model.clone()), (Some("initializing"), None, None));
    assert_eq!(a.labels, Some(Default::default()));
    let m = AgentUpdate::from_message(&pi("agent_update.model.json")).unwrap();
    assert_eq!((m.status.clone(), m.title.clone(), m.labels.clone()), (None, None, None));
    assert_eq!((m.model.as_deref(), m.model_provider.as_deref()), (Some("mock-2"), Some("mock")));

    let upd = |t: Value| AgentUpdate::from_message(&json!({"agentId":"a","title":t})).unwrap().title;
    assert_eq!(upd(json!(null)), Some(None));
    assert_eq!(upd(json!("t")), Some(Some("t".into())));
    assert_eq!(upd(json!(5)), None);
    assert_eq!(AgentUpdate::from_message(&json!({"agentId":"a"})).unwrap().title, None);
    assert!(AgentUpdate::from_message(&json!({"status":"idle"})).is_none());
    assert_eq!(upd(json!("é".repeat(400))).unwrap().unwrap().len(), 256);
}

#[test]
fn unknown_status_is_flagged() {
    let a = AgentUpdate::from_message(&message("local", "agent_update.unknown_status.json")).unwrap();
    assert_eq!(a.status.as_deref(), Some("levitating"));
    assert!(a.has_unknown_status());
    assert_eq!(AgentStatus::from_wire("levitating"), AgentStatus::Unknown);
    assert!(!AgentUpdate::from_message(&pi("agent_update.json")).unwrap().has_unknown_status());
}

#[test]
fn status_archived_deleted_and_reset() {
    let s = AgentStatusMsg::from_message(&pi("agent_status.json")).unwrap();
    assert_eq!((s.agent_id.as_str(), s.status.as_str()), ("agent-1", "idle"));
    let a = AgentArchived::from_message(&pi("agent_archived.json")).unwrap();
    assert_eq!(a.archived_at, Some(1_767_225_600_000));
    assert_eq!(AgentArchived::from_message(&json!({"agentId":"a","archivedAt":"garbage"})).unwrap().archived_at, None);
    assert_eq!(AgentArchived::from_message(&json!({"agentId":"a"})).unwrap().archived_at, None);
    assert_eq!(AgentDeleted::from_message(&pi("agent_deleted.json")).unwrap().agent_id, "00000000-0000-4000-8000-000000000001");
    let r = TimelineReset::from_message(&pi("agent_timeline_reset.json")).unwrap();
    assert_eq!(r.reason.as_deref(), Some("fork"));
    assert!(AgentDeleted::from_message(&json!({})).is_none());
}

#[test]
fn stats_fixture_and_variants() {
    let s = StatsResponse::from_message(&pi("agent_session_stats_response.json")).unwrap();
    assert!(s.ok);
    assert_eq!((s.total_messages, s.cost, s.model.as_deref()), (Some(22), None, Some("mock-model")));
    assert_eq!(s.tokens.unwrap().total, Some(0));
    assert!(s.context_usage.is_none());
    let none = StatsResponse::from_message(&json!({"requestId":"r","payload":{"ok":false,"error":"no session"}})).unwrap();
    assert!(!none.ok && none.error.as_deref() == Some("no session") && none.tokens.is_none());
    let full = StatsResponse::from_message(&json!({"requestId":"r","payload":{
        "ok":true,"tokens":{"input":100,"output":50,"cacheRead":3},"cost":0.25,"contextUsage":{"tokens":10,"contextWindow":1000,"percent":1.0}}}))
    .unwrap();
    let t = full.tokens.unwrap();
    assert_eq!((t.total, t.input, t.output, t.cache_read), (None, Some(100), Some(50), Some(3)));
    assert_eq!(full.cost, Some(0.25));
    assert_eq!(full.context_usage.unwrap().context_window, Some(1000));
    assert!(StatsResponse::from_message(&json!({"requestId":"r"})).is_none());
    assert!(StatsResponse::from_message(&json!({"payload":{}})).is_none());
}

#[test]
fn timeline_fixture_and_unknown_items() {
    let p = TimelinePage::from_message(&pi("fetch_agent_timeline_response.json")).unwrap();
    assert_eq!((p.items.len(), p.seq_end, p.has_newer), (1, 27, false));
    assert_eq!((p.start_cursor.as_deref(), p.end_cursor.as_deref()), (Some("0"), Some("27")));
    assert_eq!((p.items[0].kind.as_str(), p.items[0].source_seq, p.items[0].timestamp), ("other", 0, Some(1_767_225_600_000)));
    assert!(matches!(&p.items[0].event, StreamEvent::UserMessage { text: Some(t) } if t == "hello"));

    let mixed = json!({"requestId":"r","seqStart":0,"seqEnd":5,"hasNewer":true,"endCursor":"5","items":[
        {"kind":"other","sourceSeq":1,"event":{"kind":"hologram"}},
        {"kind":"other","sourceSeq":2},
        {"kind":"other","event":{"kind":"turn_started"}},
        {"kind":"other","sourceSeq":3,"event":{"kind":"turn_started"}}]});
    let p = TimelinePage::from_message(&mixed).unwrap();
    assert_eq!((p.items.len(), p.malformed, p.unknown_kinds, p.has_newer), (2, 2, 1, true));
    assert!(TimelinePage::from_message(&json!({"requestId":"r","items":[],"seqEnd":1})).is_none());
}

/// The shape a real daemon returns: `assistant` and `tool_call` runs are collapsed into one item
/// with `events[]` and a `sourceSeqStart..End` range (pi-studio `timeline-store.ts`).
#[test]
fn collapsed_items_are_flattened_to_their_events() {
    let page = json!({"requestId":"r","seqStart":0,"seqEnd":9,"hasNewer":false,"items":[
        {"kind":"other","sourceSeq":0,"event":{"kind":"user_message","text":"hi"}},
        {"kind":"assistant","sourceSeqStart":2,"sourceSeqEnd":4,"timestamp":"2026-01-01T00:00:00Z","events":[
            {"kind":"assistant_message","text":"a "},{"kind":"reasoning","text":"r"},{"kind":"assistant_message","text":"b","final":true}]},
        {"kind":"tool_call","callId":"c1","sourceSeqStart":5,"sourceSeqEnd":6,"timestamp":"2026-01-01T00:00:00Z","events":[
            {"kind":"tool_call","callId":"c1","status":"running","tool":{"kind":"shell","command":"ls"}},
            {"kind":"tool_call","callId":"c1","status":"completed","tool":{"kind":"shell","command":"ls","output":"x"}}]},
        {"kind":"assistant","sourceSeqStart":7,"sourceSeqEnd":7,"events":[]},
        {"kind":"assistant","events":[{"kind":"assistant_message","text":"no seq"}]}]});
    let p = TimelinePage::from_message(&page).unwrap();
    let kinds: Vec<(&str, u64)> = p.items.iter().map(|i| (i.kind.as_str(), i.source_seq)).collect();
    assert_eq!(kinds, [("other", 0), ("assistant", 4), ("assistant", 4), ("assistant", 4), ("tool_call", 6), ("tool_call", 6)]);
    assert!(matches!(&p.items[3].event, StreamEvent::AssistantMessage { is_final: true, .. }));
    assert_eq!(p.malformed, 2, "an empty group and a group without a sequence are malformed");
}
