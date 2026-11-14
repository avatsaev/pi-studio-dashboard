#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{kind, live_state, run, snapshot, stream};
use pistudiodashboard_ingest::{DrillError, drill_deselect, drill_select};
use pistudiodashboard_model::{ConnState, DaemonEventKind, DrillItem, DrillState, Effect, MonitorState, StreamEvent, TimelineItem, ToolInfo, TurnKind};

fn drilled() -> MonitorState {
    let mut s = live_state(&[("a1", "running"), ("a2", "idle")]);
    let fx = drill_select(&mut s, "srv", "a1").unwrap();
    assert_eq!(fx, [Effect::FetchTimeline { server_id: "srv".into(), agent_id: "a1".into() }]);
    s
}

fn buf(s: &MonitorState) -> &pistudiodashboard_model::DrillBuffer {
    s.drilldown.as_ref().unwrap()
}

fn items(s: &MonitorState) -> Vec<DrillItem> {
    buf(s).items.iter().cloned().collect()
}

fn page(agent: &str, events: Vec<StreamEvent>, reset: bool, seq_end: u64) -> pistudiodashboard_model::DaemonEvent {
    let items = events.into_iter().enumerate().map(|(i, event)| TimelineItem { kind: "other".into(), source_seq: i as u64, timestamp: None, event }).collect();
    kind(DaemonEventKind::Timeline { agent_id: agent.into(), items, reset, seq_end })
}

fn delta(text: &str) -> StreamEvent {
    StreamEvent::AssistantMessage { text: text.into(), is_final: false }
}

fn tool(call: &str, status: &str, partial: bool, output: Option<&str>) -> StreamEvent {
    StreamEvent::ToolCall {
        call_id: call.into(),
        status: status.into(),
        partial,
        tool: ToolInfo { kind: "shell".into(), detail: Some("npm test".into()), output: output.map(Into::into) },
    }
}

#[test]
fn select_requires_an_existing_unarchived_agent_and_replaces_the_previous_buffer() {
    let mut s = live_state(&[("a1", "running"), ("a2", "idle")]);
    assert_eq!(drill_select(&mut s, "srv", "nope"), Err(DrillError::NotAvailable));
    assert!(s.drilldown.is_none());
    drill_select(&mut s, "srv", "a1").unwrap();
    run(&mut s, &[page("a1", vec![StreamEvent::TurnStarted], true, 1)], 1);
    assert_eq!(buf(&s).items.len(), 1);
    drill_select(&mut s, "srv", "a2").unwrap();
    assert_eq!((buf(&s).target.agent_id.as_str(), buf(&s).items.len(), buf(&s).state), ("a2", 0, DrillState::Loading));
    run(&mut s, &[kind(DaemonEventKind::AgentArchived { agent_id: "a1".into(), archived_at: None })], 2);
    assert_eq!(drill_select(&mut s, "srv", "a1"), Err(DrillError::NotAvailable), "archived");
    assert!(drill_deselect(&mut s) && s.drilldown.is_none());
    assert!(!drill_deselect(&mut s));
}

#[test]
fn the_first_page_loads_the_buffer_and_replays_only_events_newer_than_seq_end() {
    let mut s = drilled();
    run(&mut s, &[stream("a1", Some(8), delta("old ")), stream("a1", Some(11), delta("new")), stream("a1", None, delta("!"))], 1);
    assert!(buf(&s).items.is_empty(), "held while loading");
    assert_eq!(buf(&s).held.len(), 3);
    run(&mut s, &[page("a1", vec![StreamEvent::UserMessage { text: Some("hi".into()) }, delta("page ")], true, 10)], 2);
    let b = buf(&s);
    assert_eq!((b.state, b.seq_high, b.held.len(), b.stale), (DrillState::Live, Some(11), 0, false));
    assert_eq!(
        items(&s),
        [DrillItem::System { text: "hi".into() }, DrillItem::Assistant { text: "page new!".into() }],
        "seq 8 <= 10 dropped; seq 11 and the seq-less delta applied to the open block"
    );
    run(&mut s, &[stream("a1", Some(11), delta("dup")), stream("a1", Some(12), delta("?"))], 3);
    assert_eq!(items(&s)[1], DrillItem::Assistant { text: "page new!?".into() });
}

#[test]
fn text_blocks_follow_final_and_kind_changes() {
    let mut s = drilled();
    run(&mut s, &[page("a1", vec![], true, 0)], 1);
    let events = [
        stream("a1", Some(1), delta("a")),
        stream("a1", Some(2), delta("b")),
        stream("a1", Some(3), StreamEvent::AssistantMessage { text: String::new(), is_final: true }),
        stream("a1", Some(4), delta("c")),
        stream("a1", Some(5), StreamEvent::Reasoning { text: Some("r".into()), is_final: false }),
        stream("a1", Some(6), StreamEvent::Reasoning { text: Some("s".into()), is_final: true }),
        stream("a1", Some(7), StreamEvent::Reasoning { text: Some("t".into()), is_final: false }),
    ];
    run(&mut s, &events, 2);
    assert_eq!(
        items(&s),
        [
            DrillItem::Assistant { text: "ab".into() },
            DrillItem::Assistant { text: "c".into() },
            DrillItem::Reasoning { text: "rs".into() },
            DrillItem::Reasoning { text: "t".into() },
        ]
    );
}

#[test]
fn tool_calls_upsert_by_call_id_with_a_twelve_line_tail_and_close_on_failure() {
    let mut s = drilled();
    run(&mut s, &[page("a1", vec![], true, 0)], 1);
    let many: String = (0..30).map(|i| format!("line{i}\n")).collect();
    run(
        &mut s,
        &[
            stream("a1", Some(1), tool("c1", "started", false, None)),
            stream("a1", None, tool("c1", "running", true, Some(&many))),
            stream("a1", Some(2), tool("c2", "running", false, None)),
        ],
        2,
    );
    let tools: Vec<_> = items(&s);
    assert_eq!(tools.len(), 2, "c1 updated in place");
    let DrillItem::Tool { status, output_tail: Some(tail), label, .. } = &tools[0] else { panic!("{:?}", tools[0]) };
    assert_eq!((status.as_str(), label.as_str(), tail.len()), ("running", "npm test", 12));
    assert_eq!((tail[0].as_str(), tail[11].as_str()), ("line18", "line29"));
    run(&mut s, &[stream("a1", Some(3), tool("c1", "completed", false, Some("done\nok")))], 3);
    let DrillItem::Tool { status, output_tail: Some(tail), .. } = &items(&s)[0] else { panic!() };
    assert_eq!((status.as_str(), tail.clone()), ("completed", vec!["done".to_owned(), "ok".to_owned()]), "terminal output is authoritative");
    run(&mut s, &[stream("a1", Some(4), StreamEvent::TurnFailed { error: Some("boom".into()) })], 4);
    let all = items(&s);
    assert!(matches!(&all[1], DrillItem::Tool { status, .. } if status == "error"), "the running tool closes as error");
    assert!(matches!(&all[0], DrillItem::Tool { status, .. } if status == "completed"));
    assert_eq!(all[all.len() - 2..], [DrillItem::Turn { kind: TurnKind::Failed }, DrillItem::Error { message: "boom".into() }]);
    run(&mut s, &[stream("a1", Some(5), tool("c3", "running", false, None)), stream("a1", Some(6), StreamEvent::TurnCanceled)], 5);
    assert!(
        matches!(items(&s).iter().find(|i| matches!(i, DrillItem::Tool { call_id, .. } if call_id == "c3")), Some(DrillItem::Tool { status, .. }) if status == "canceled")
    );
}

#[test]
fn five_thousand_items_keep_the_newest_two_thousand() {
    let mut s = drilled();
    run(&mut s, &[page("a1", vec![], true, 0)], 1);
    let events: Vec<_> = (0..5000).map(|i| stream("a1", Some(i + 1), StreamEvent::Error { message: Some(format!("e{i}")) })).collect();
    run(&mut s, &events, 2);
    let b = buf(&s);
    assert_eq!(b.items.len(), 2000);
    assert!(b.truncated_at_start);
    assert_eq!(b.items.last(), Some(&DrillItem::Error { message: "e4999".into() }));
    assert!(s.validate().is_ok());
}

#[test]
fn timeline_reset_clears_and_refetches_then_a_reset_page_refills() {
    let mut s = drilled();
    run(&mut s, &[page("a1", vec![StreamEvent::TurnStarted], true, 5)], 1);
    let mut fx = Vec::new();
    pistudiodashboard_ingest::apply_in_place(&mut s, &kind(DaemonEventKind::TimelineReset { agent_id: "a1".into(), reason: Some("fork".into()) }), 2, &mut fx);
    assert_eq!(fx, [Effect::FetchTimeline { server_id: "srv".into(), agent_id: "a1".into() }]);
    assert_eq!((buf(&s).state, buf(&s).items.len(), buf(&s).seq_high), (DrillState::Refetching, 0, None));
    run(&mut s, &[stream("a1", Some(1), delta("live"))], 3);
    assert!(buf(&s).items.is_empty(), "held while refetching");
    run(&mut s, &[page("a1", vec![StreamEvent::TurnStarted], true, 0)], 4);
    assert_eq!((buf(&s).state, buf(&s).items.len()), (DrillState::Live, 2));
    // Another agent's reset or page does not touch this buffer.
    let mut fx = Vec::new();
    pistudiodashboard_ingest::apply_in_place(&mut s, &kind(DaemonEventKind::TimelineReset { agent_id: "a2".into(), reason: None }), 5, &mut fx);
    assert!(fx.is_empty());
    run(&mut s, &[page("a2", vec![StreamEvent::TurnStarted], true, 9)], 6);
    assert_eq!(buf(&s).items.len(), 2);
}

#[test]
fn later_pages_append_and_unknown_items_are_skipped_and_counted() {
    let mut s = drilled();
    run(&mut s, &[page("a1", vec![StreamEvent::TurnStarted, StreamEvent::Unknown { kind: "x".into() }], true, 3)], 1);
    assert_eq!((buf(&s).items.len(), s.dropped_events), (1, 1));
    run(&mut s, &[page("a1", vec![StreamEvent::TurnCompleted], false, 8)], 2);
    assert_eq!((buf(&s).items.len(), buf(&s).seq_high), (2, Some(8)));
}

#[test]
fn disconnect_keeps_the_buffer_read_only_and_reconnect_refetches_or_closes() {
    let mut s = drilled();
    run(&mut s, &[page("a1", vec![StreamEvent::TurnStarted], true, 1)], 1);
    run(&mut s, &[common::conn("e1", 1, ConnState::Backoff)], 2);
    assert!(buf(&s).stale && buf(&s).items.len() == 1);
    let fx = run(&mut s, &[common::conn("e1", 2, ConnState::Live), snapshot("e1", 2, vec![common::item("a1", "running")], vec![])], 3);
    assert!(fx.contains(&Effect::FetchTimeline { server_id: "srv".into(), agent_id: "a1".into() }));
    assert_eq!((buf(&s).state, buf(&s).stale), (DrillState::Refetching, false));
    run(&mut s, &[common::conn("e1", 3, ConnState::Live), snapshot("e1", 3, vec![common::item("a2", "idle")], vec![])], 4);
    assert!(s.drilldown.is_none(), "the agent vanished from the new snapshot");
}
