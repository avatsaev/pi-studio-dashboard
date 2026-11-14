//! The event JSON the app feeds into `feed.ingest`: every new kind round-trips, and the partial
//! `agent_update` keeps "absent", `null` and a value apart.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pistudiodashboard_model::{ContextUsage, DaemonEvent, DaemonEventKind, StatsResult, StreamEvent, TimelineItem, TokenUsage, ToolInfo};
use serde_json::{Value, json};

fn event(kind: DaemonEventKind) -> DaemonEvent {
    DaemonEvent { entry_id: "e1".into(), server_id: Some("srv".into()), conn_epoch: 2, seq: 7, at: 99, kind }
}

fn round_trip(kind: DaemonEventKind) {
    let e = event(kind);
    let json = serde_json::to_string(&e).unwrap();
    let back: DaemonEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back, e, "{json}");
}

fn update(title: Option<Option<String>>) -> DaemonEventKind {
    DaemonEventKind::AgentUpdate { agent_id: "a".into(), status: None, title, labels: None, model: None, model_provider: None }
}

#[test]
fn every_agent_event_kind_round_trips() {
    round_trip(DaemonEventKind::AgentUpdate {
        agent_id: "a".into(),
        status: Some("running".into()),
        title: Some(Some("t".into())),
        labels: Some(vec!["k=v".into()]),
        model: Some("m".into()),
        model_provider: None,
    });
    round_trip(update(Some(None)));
    round_trip(update(None));
    round_trip(DaemonEventKind::AgentStatus { agent_id: "a".into(), status: "idle".into() });
    round_trip(DaemonEventKind::AgentArchived { agent_id: "a".into(), archived_at: Some(5) });
    round_trip(DaemonEventKind::AgentDeleted { agent_id: "a".into() });
    round_trip(DaemonEventKind::TimelineReset { agent_id: "a".into(), reason: Some("fork".into()) });
    for ev in [
        StreamEvent::UserMessage { text: Some("hi".into()) },
        StreamEvent::AssistantMessage { text: "x".into(), is_final: true },
        StreamEvent::Reasoning { text: None, is_final: false },
        StreamEvent::ToolCall {
            call_id: "c".into(),
            status: "running".into(),
            partial: true,
            tool: ToolInfo { kind: "shell".into(), detail: Some("ls".into()), output: None },
        },
        StreamEvent::TurnStarted,
        StreamEvent::TurnFailed { error: Some("e".into()) },
        StreamEvent::Compaction { phase: Some("started".into()), reason: None },
        StreamEvent::Unknown { kind: "x".into() },
    ] {
        round_trip(DaemonEventKind::AgentStream { agent_id: "a".into(), seq: Some(3), timestamp: Some(1), event: ev.clone() });
        round_trip(DaemonEventKind::AgentStream { agent_id: "a".into(), seq: None, timestamp: None, event: ev });
    }
    round_trip(DaemonEventKind::Stats { agent_id: "a".into(), result: StatsResult::NoSession });
    round_trip(DaemonEventKind::Stats {
        agent_id: "a".into(),
        result: StatsResult::Usage {
            tokens: Some(TokenUsage { input: Some(1), output: Some(2), total: None }),
            cost: Some(0.5),
            context_usage: Some(ContextUsage { tokens: Some(1), window: Some(2), percent: Some(50.0) }),
        },
    });
    round_trip(DaemonEventKind::Timeline {
        agent_id: "a".into(),
        items: vec![TimelineItem { kind: "other".into(), source_seq: 4, timestamp: Some(2), event: StreamEvent::TurnCompleted }],
        reset: true,
        seq_end: 9,
    });
}

#[test]
fn a_partial_update_distinguishes_absent_null_and_value_in_json() {
    let parse = |title: Option<Value>| {
        let mut k = json!({"type": "agentUpdate", "agentId": "a"});
        if let Some(t) = title {
            k["title"] = t;
        }
        match serde_json::from_value::<DaemonEventKind>(k).unwrap() {
            DaemonEventKind::AgentUpdate { title, .. } => title,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(parse(None), None);
    assert_eq!(parse(Some(Value::Null)), Some(None));
    assert_eq!(parse(Some(json!("x"))), Some(Some("x".into())));
    let absent = serde_json::to_value(update(None)).unwrap();
    assert!(absent.get("title").is_none(), "an absent title is not serialised: {absent}");
}
