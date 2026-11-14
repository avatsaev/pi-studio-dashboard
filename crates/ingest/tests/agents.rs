#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;

use common::{kind, live_state, run, snapshot, stream, update};
use pistudiodashboard_ingest::{Filters, card_order};
use pistudiodashboard_model::{
    AgentStatus, DaemonEvent, DaemonEventKind, DrillBuffer, DrillTarget, Effect, MonitorState, NotifyKind, StreamEvent, TurnState, agent_key,
};
use pistudiodashboard_protocol::{AgentStream, ToolInfo};
use serde_json::Value;

fn fixture_stream(name: &str) -> DaemonEvent {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../protocol/tests/fixtures/pi-studio").join(name);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let s = AgentStream::from_message(&v["frame"]["message"]).unwrap();
    stream("00000000-0000-4000-8000-000000000001", s.seq, s.event)
}

const A1: &str = "00000000-0000-4000-8000-000000000001";

fn agent<'a>(s: &'a MonitorState, id: &str) -> &'a pistudiodashboard_model::AgentEntry {
    s.agent("srv", id).unwrap()
}

fn notifies(fx: &[Effect]) -> Vec<&Effect> {
    fx.iter().filter(|e| matches!(e, Effect::Notify { .. })).collect()
}

#[test]
fn golden_replay_of_the_vendored_stream_fixtures() {
    let mut s = live_state(&[(A1, "running")]);
    run(&mut s, &[fixture_stream("agent_stream.turn_started.json")], 200);
    assert_eq!(agent(&s, A1).activity.turn_state, TurnState::Started);
    run(&mut s, &[fixture_stream("agent_stream.tool_call.json")], 200);
    let tool = agent(&s, A1).activity.current_tool.clone().unwrap();
    assert_eq!((tool.kind.as_str(), tool.label.as_str(), tool.status.as_str()), ("shell", "npm test", "running"));
    run(&mut s, &[fixture_stream("agent_stream.tool_call.partial.json")], 200);
    assert_eq!(agent(&s, A1).activity.last_output_line.as_deref(), Some("> running suite A"));
    run(
        &mut s,
        &[fixture_stream("agent_stream.reasoning.json"), fixture_stream("agent_stream.user_message.json"), fixture_stream("agent_stream.compaction.json")],
        200,
    );
    assert_eq!(agent(&s, A1).activity.last_output_line.as_deref(), Some("> running suite A"), "reasoning, user message and compaction never touch the line");
    assert!(agent(&s, A1).activity.current_tool.is_some());
    let fx = run(&mut s, &[fixture_stream("agent_stream.turn_completed.json")], 200);
    assert!(agent(&s, A1).activity.current_tool.is_none());
    assert_eq!(agent(&s, A1).activity.turn_state, TurnState::Completed);
    assert!(fx.contains(&Effect::FetchStats { server_id: "srv".into(), agent_id: A1.into() }));

    let fx = run(&mut s, &[fixture_stream("agent_stream.turn_failed.json")], 200);
    let a = agent(&s, A1);
    assert_eq!((a.activity.turn_state, a.attention.errored, a.activity.last_error_message.as_deref()), (TurnState::Failed, true, Some("mock turn failure")));
    assert_eq!(notifies(&fx).len(), 1);
    assert!(matches!(notifies(&fx)[0], Effect::Notify { kind: NotifyKind::Failures, item_id, .. } if item_id == &format!("fail:{A1}")));
    assert_eq!(s.daemons[0].events.iter().filter(|e| e.summary == "turn failed").count(), 1);
    assert!(s.validate().is_ok());
}

#[test]
fn a_mid_stream_error_sets_the_message_and_notifies_without_marking_the_agent_errored() {
    let mut s = live_state(&[(A1, "running")]);
    let fx = run(&mut s, &[fixture_stream("agent_stream.error.json")], 200);
    let a = agent(&s, A1);
    assert_eq!((a.activity.last_error_message.as_deref(), a.attention.errored), (Some("mock stream error"), false));
    assert_eq!(notifies(&fx).len(), 1);
}

#[test]
fn status_transitions_clear_the_tool_and_bump_activity_only_for_status_and_title() {
    let mut s = live_state(&[(A1, "idle")]);
    run(&mut s, &[update(A1, Some("running"))], 500);
    assert_eq!((agent(&s, A1).status, agent(&s, A1).last_activity), (AgentStatus::Running, Some(500)));
    run(&mut s, &[fixture_stream("agent_stream.tool_call.json")], 900);
    assert!(agent(&s, A1).activity.current_tool.is_some());
    assert_eq!(agent(&s, A1).last_activity, Some(500), "stream events never bump lastActivity");
    run(&mut s, &[update(A1, Some("idle"))], 700);
    assert!(agent(&s, A1).activity.current_tool.is_none(), "leaving running clears the tool");
    assert_eq!(agent(&s, A1).last_activity, Some(700));
    run(&mut s, &[update(A1, Some("idle"))], 800);
    assert_eq!(agent(&s, A1).last_activity, Some(700), "no change, no bump");
}

#[test]
fn error_status_flags_notifies_once_and_clears_on_recovery() {
    let mut s = live_state(&[(A1, "running")]);
    let fx = run(&mut s, &[update(A1, Some("error"))], 10);
    assert!(agent(&s, A1).attention.errored);
    assert_eq!(notifies(&fx).len(), 1);
    assert_eq!(s.daemons[0].events.iter().filter(|e| e.summary == "agent error").count(), 1);
    let fx = run(&mut s, &[update(A1, Some("error"))], 20);
    assert!(notifies(&fx).is_empty(), "already in error");
    run(&mut s, &[update(A1, Some("idle"))], 30);
    assert!(!agent(&s, A1).attention.errored);
}

#[test]
fn partial_updates_keep_absent_fields_and_null_clears_the_title() {
    let mut s = live_state(&[(A1, "idle")]);
    assert_eq!(agent(&s, A1).title.as_deref(), Some(&*format!("t-{A1}")));
    let patch = |title: Option<Option<&str>>| {
        kind(DaemonEventKind::AgentUpdate {
            agent_id: A1.into(),
            status: None,
            title: title.map(|t| t.map(Into::into)),
            labels: None,
            model: Some("m2".into()),
            model_provider: None,
        })
    };
    run(&mut s, &[patch(None)], 50);
    assert_eq!((agent(&s, A1).title.is_some(), agent(&s, A1).model.as_deref(), agent(&s, A1).last_activity), (true, Some("m2"), None));
    run(&mut s, &[patch(Some(None))], 60);
    assert_eq!((agent(&s, A1).title.clone(), agent(&s, A1).last_activity), (None, Some(60)));
    run(&mut s, &[patch(Some(Some(&"x".repeat(1_000_000))))], 70);
    assert!(agent(&s, A1).title.as_ref().unwrap().len() <= 200);
    assert!(s.validate().is_ok());
}

#[test]
fn an_unknown_agent_gets_a_placeholder_and_stats_while_unknown_streams_are_ignored_and_counted() {
    let mut s = live_state(&[]);
    let fx = run(&mut s, &[update("ghost", Some("running"))], 10);
    assert_eq!(agent(&s, "ghost").status, AgentStatus::Running);
    assert!(fx.contains(&Effect::FetchStats { server_id: "srv".into(), agent_id: "ghost".into() }));
    let before = s.dropped_events;
    let fx = run(&mut s, &[stream("nobody", Some(1), StreamEvent::TurnStarted)], 10);
    assert!(s.agent("srv", "nobody").is_none() && fx.is_empty());
    assert_eq!(s.dropped_events, before + 1);
}

fn dialog_snapshot(agents: &[&str]) -> DaemonEvent {
    let items = agents.iter().map(|a| common::item(a, "running")).collect();
    snapshot("e1", 1, items, vec![common::pending("r1", "a1"), common::pending("r2", "a2")])
}

#[test]
fn archived_drops_dialogs_and_closes_the_drill_only_for_its_target() {
    let mut s = live_state(&[]);
    run(&mut s, &[dialog_snapshot(&["a1", "a2"])], 10);
    assert_eq!(s.ui_requests.len(), 2);
    s.drilldown = Some(DrillBuffer { target: DrillTarget { server_id: "srv".into(), agent_id: "a2".into() }, ..DrillBuffer::default() });
    let fx = run(&mut s, &[kind(DaemonEventKind::AgentArchived { agent_id: "a1".into(), archived_at: Some(5) })], 20);
    assert!(agent(&s, "a1").archived && s.ui_request("srv", "r1").is_none() && s.ui_request("srv", "r2").is_some());
    assert!(fx.is_empty() && s.drilldown.is_some(), "not the drill target");
    let fx = run(&mut s, &[kind(DaemonEventKind::AgentArchived { agent_id: "a2".into(), archived_at: None })], 30);
    assert_eq!(fx, [Effect::CloseDrill { reason: "archived".into() }]);
    assert!(s.drilldown.is_none());
}

#[test]
fn deleted_removes_the_entry_and_closes_the_drill() {
    let mut s = live_state(&[]);
    run(&mut s, &[dialog_snapshot(&["a1", "a2"])], 10);
    s.drilldown = Some(DrillBuffer { target: DrillTarget { server_id: "srv".into(), agent_id: "a1".into() }, ..DrillBuffer::default() });
    let fx = run(&mut s, &[kind(DaemonEventKind::AgentDeleted { agent_id: "a1".into() })], 20);
    assert!(!s.agents.contains_key(&agent_key("srv", "a1")) && s.ui_request("srv", "r1").is_none());
    assert_eq!(fx, [Effect::CloseDrill { reason: "deleted".into() }]);
    run(&mut s, &[kind(DaemonEventKind::AgentDeleted { agent_id: "never-existed".into() })], 21);
}

#[test]
fn closed_agents_rank_last_and_attention_first_and_stream_order_does_not_matter() {
    let mut s = live_state(&[("a1", "idle"), ("a2", "running"), ("a3", "closed"), ("a4", "running")]);
    run(&mut s, &[update("a1", Some("error"))], 5);
    let order = |s: &MonitorState| card_order(s, &Filters::default()).into_iter().map(|(_, a)| a).collect::<Vec<_>>();
    assert_eq!(order(&s), ["a1", "a2", "a4", "a3"], "errored (attention) first, running, closed last");
    let deltas = [
        stream("a2", None, StreamEvent::AssistantMessage { text: "x".into(), is_final: false }),
        stream("a4", Some(1), StreamEvent::TurnStarted),
        stream("a2", Some(2), StreamEvent::TurnCompleted),
        stream("a4", None, StreamEvent::Reasoning { text: Some("t".into()), is_final: false }),
    ];
    let (mut fwd, mut rev) = (s.clone(), s.clone());
    run(&mut fwd, &deltas, 9);
    let reversed: Vec<DaemonEvent> = deltas.iter().rev().cloned().collect();
    run(&mut rev, &reversed, 9);
    assert_eq!(order(&fwd), order(&rev));
    assert_eq!(order(&fwd), order(&s));
}

#[test]
fn summary_fields_respect_their_caps_for_megabyte_inputs() {
    let mut s = live_state(&[(A1, "running")]);
    let big = "é".repeat(500_000);
    let events = [
        stream(A1, Some(1), StreamEvent::AssistantMessage { text: big.clone(), is_final: false }),
        stream(
            A1,
            Some(2),
            StreamEvent::ToolCall {
                call_id: "c".into(),
                status: "running".into(),
                partial: true,
                tool: ToolInfo { kind: "shell".into(), detail: Some(big.clone()), output: Some(big.clone()) },
            },
        ),
        stream(A1, Some(3), StreamEvent::TurnFailed { error: Some(big) }),
    ];
    run(&mut s, &events, 1);
    let a = agent(&s, A1);
    assert!(a.assistant_tail.len() <= 1024);
    assert!(a.activity.last_output_line.as_ref().unwrap().len() <= 200);
    assert!(a.activity.current_tool.as_ref().is_none_or(|t| t.label.len() <= 120));
    assert!(a.activity.last_error_message.as_ref().unwrap().len() <= 200);
    assert!(s.validate().is_ok(), "{:?}", s.validate());
}
