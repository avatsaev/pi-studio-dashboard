//! Agent pushes, stats executor and timeline pager against the scripted fake daemon.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use pistudiodashboard_client::{ClientConfig, ClientHandle, DaemonEvent, DaemonEventKind, OutboundRequest, StatsResult, StreamEvent};
use pistudiodashboard_model::{ConnState, CountersSnapshot};
use serde_json::{Value, json};
use support::fake_daemon::{After, Behavior, FakeDaemon, Outgoing, inner_type};
use support::{Events, collect_until, entry, fixture_frame, is_state};

fn start_with(capacity: usize) -> (ClientHandle, Events) {
    let mut config = ClientConfig::new("0.1.0");
    config.event_channel_capacity = capacity;
    let (client, rx) = ClientHandle::start_in(tokio::runtime::Handle::current(), config);
    (client, Events::new(rx))
}

fn is_snapshot(e: &DaemonEvent) -> bool {
    matches!(e.kind, DaemonEventKind::Snapshot { .. })
}

fn diagnostics(e: &DaemonEvent) -> Option<&CountersSnapshot> {
    match &e.kind {
        DaemonEventKind::Diagnostics(c) => Some(c),
        _ => None,
    }
}

fn session(m: Value) -> String {
    json!({"type": "session", "message": m}).to_string()
}

async fn live(daemon: &FakeDaemon) -> (ClientHandle, Events) {
    let (client, mut rx) = start_with(1024);
    client.connect(entry("e1", daemon.port()), None).unwrap();
    collect_until(&mut rx, is_snapshot).await;
    (client, rx)
}

#[tokio::test]
async fn every_agent_push_becomes_its_event_after_the_snapshot_in_arrival_order() {
    let names = [
        "agent_update.json",
        "agent_update.model.json",
        "agent_status.json",
        "agent_archived.json",
        "agent_deleted.json",
        "agent_timeline_reset.json",
        "agent_stream.user_message.json",
        "agent_stream.tool_call.partial.json",
        "agent_stream.turn_failed.json",
    ];
    let mut frames: Vec<Outgoing> = names.iter().map(|n| Outgoing::Text(fixture_frame("pi-studio", n))).collect();
    frames.push(Outgoing::Text(fixture_frame("local", "agent_update.unknown_status.json")));
    frames.push(Outgoing::Text(fixture_frame("local", "agent_stream.unknown_kind.json")));
    let total = frames.len();
    let daemon = FakeDaemon::start(Behavior { after: After::Send(frames), ..Behavior::default() }).await;
    let (client, mut rx) = start_with(1024);
    client.connect(entry("e1", daemon.port()), None).unwrap();
    let mut events = collect_until(&mut rx, is_snapshot).await;
    let mut pushes = Vec::new();
    while pushes.len() < total {
        events.extend(collect_until(&mut rx, |e| !matches!(e.kind, DaemonEventKind::Diagnostics(_))).await);
        pushes = events.iter().skip_while(|e| !is_snapshot(e)).skip(1).filter(|e| diagnostics(e).is_none()).cloned().collect();
    }
    let kinds: Vec<&DaemonEventKind> = pushes.iter().map(|e| &e.kind).collect();
    let DaemonEventKind::AgentUpdate { status, title, labels, .. } = kinds[0] else { panic!("{:?}", kinds[0]) };
    assert_eq!((status.as_deref(), title.clone(), labels.clone()), (Some("initializing"), None, Some(vec![])));
    assert!(matches!(kinds[1], DaemonEventKind::AgentUpdate { model: Some(m), status: None, .. } if m == "mock-2"));
    assert!(matches!(kinds[2], DaemonEventKind::AgentStatus { status, .. } if status == "idle"));
    assert!(matches!(kinds[3], DaemonEventKind::AgentArchived { archived_at: Some(1_767_225_600_000), .. }));
    assert!(matches!(kinds[4], DaemonEventKind::AgentDeleted { .. }));
    assert!(matches!(kinds[5], DaemonEventKind::TimelineReset { reason: Some(r), .. } if r == "fork"));
    assert!(matches!(kinds[6], DaemonEventKind::AgentStream { seq: Some(0), event: StreamEvent::UserMessage { .. }, .. }));
    assert!(matches!(kinds[7], DaemonEventKind::AgentStream { seq: None, event: StreamEvent::ToolCall { partial: true, .. }, .. }));
    assert!(matches!(kinds[8], DaemonEventKind::AgentStream { event: StreamEvent::TurnFailed { .. }, .. }));
    assert!(matches!(kinds[9], DaemonEventKind::AgentUpdate { status: Some(s), .. } if s == "levitating"), "unknown status is forwarded raw");
    assert!(matches!(kinds[10], DaemonEventKind::AgentStream { event: StreamEvent::Unknown { kind }, .. } if kind == "hologram_projected"));
    // Both unknowns are counted; nothing is malformed; the connection stays live.
    let diag = collect_until(&mut rx, |e| diagnostics(e).is_some_and(|c| c.unknown_enum_values >= 2)).await;
    let c = diagnostics(diag.last().unwrap()).unwrap();
    assert_eq!((c.unknown_enum_values, c.malformed), (2, 0));
    assert!(!events.iter().any(|e| is_state(e, ConnState::Backoff)));
    drop(client);
}

fn delta(n: usize) -> Outgoing {
    Outgoing::Text(session(json!({"type": "agent_stream", "agentId": "a1", "event": {"kind": "assistant_message", "text": format!("w{n} ")}})))
}

fn attention(kind: &str, seq: u64) -> Outgoing {
    Outgoing::Text(session(json!({"type": "agent_stream", "agentId": "a1", "seq": seq, "event": {"kind": kind}})))
}

#[tokio::test]
async fn a_stalled_consumer_sheds_only_foldable_deltas_and_never_attention_events() {
    const DELTAS: usize = 50_000;
    let mut frames = Vec::new();
    let mut expected = 0;
    for i in 0..DELTAS {
        if i % 5000 == 0 {
            frames.push(attention("turn_started", i as u64));
            frames.push(attention("turn_failed", i as u64 + 1));
            expected += 2;
        }
        frames.push(delta(i));
    }
    let daemon = FakeDaemon::start(Behavior { after: After::Send(frames), ..Behavior::default() }).await;
    let (client, mut rx) = start_with(1);
    client.connect(entry("e1", daemon.port()), None).unwrap();
    // The consumer is stalled while the daemon streams.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let (mut turn_events, mut deltas, mut dropped) = (0_usize, 0_usize, 0_u64);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while tokio::time::Instant::now() < deadline {
        let Ok(Some(batch)) = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await else { break };
        for e in batch {
            match &e.kind {
                DaemonEventKind::AgentStream { event: StreamEvent::AssistantMessage { .. }, .. } => deltas += 1,
                DaemonEventKind::AgentStream { event: StreamEvent::TurnStarted | StreamEvent::TurnFailed { .. }, .. } => turn_events += 1,
                DaemonEventKind::Diagnostics(c) => dropped = c.dropped_events,
                _ => {}
            }
        }
        if turn_events == expected && deltas as u64 + dropped == DELTAS as u64 {
            break;
        }
    }
    assert_eq!(turn_events, expected, "no attention-class event was dropped");
    assert_eq!(deltas as u64 + dropped, DELTAS as u64, "droppedEvents counts exactly the shed deltas (delivered {deltas}, dropped {dropped})");
    assert!(dropped > 0, "a stalled consumer must cause shedding");
    drop(client);
}

fn stats_response(request_id: &Value, ok: bool) -> String {
    let payload = if ok { json!({"ok": true, "tokens": {"input": 100, "output": 50}, "cost": 0.5}) } else { json!({"ok": false, "error": "no session"}) };
    session(json!({"type": "agent_session_stats_response", "requestId": request_id, "payload": payload}))
}

fn stats_requests(daemon: &FakeDaemon) -> Vec<Value> {
    daemon.received().into_iter().filter(|f| inner_type(f) == "agent_session_stats_request").collect()
}

async fn wait_until(mut cond: impl FnMut() -> bool) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not reached");
}

#[tokio::test]
async fn stats_are_deduplicated_per_agent_limited_to_four_in_flight_and_answered_as_events() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = live(&daemon).await;
    for i in 0..10 {
        client.request("e1", OutboundRequest::SessionStats { agent_id: format!("a{i}") }, "").unwrap();
    }
    client.request("e1", OutboundRequest::SessionStats { agent_id: "a0".into() }, "").unwrap(); // duplicate
    wait_until(|| stats_requests(&daemon).len() == 4).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(stats_requests(&daemon).len(), 4, "at most 4 in flight, duplicates dropped");

    let mut answered = 0;
    let mut results = Vec::new();
    while answered < 10 {
        let sent = stats_requests(&daemon);
        assert!(sent.len() - answered <= 4, "never more than 4 unanswered");
        for (i, req) in sent.iter().enumerate().skip(answered) {
            let agent = req["message"]["agentId"].as_str().unwrap();
            let ok = !agent.ends_with('1');
            daemon.push(stats_response(&req["message"]["requestId"], ok));
            let _ = i;
        }
        let batch_end = sent.len();
        let events = support::collect_count(&mut rx, batch_end - answered, |e| matches!(e.kind, DaemonEventKind::Stats { .. })).await;
        results.extend(events.into_iter().filter_map(|e| match e.kind {
            DaemonEventKind::Stats { agent_id, result } => Some((agent_id, result)),
            _ => None,
        }));
        answered = batch_end;
        if answered < 10 {
            wait_until(|| stats_requests(&daemon).len() > answered).await;
        }
    }
    assert_eq!(results.len(), 10);
    let a0 = results.iter().find(|(a, _)| a == "a0").unwrap();
    assert!(matches!(&a0.1, StatsResult::Usage { tokens: Some(t), cost: Some(c), .. } if t.input == Some(100) && t.total.is_none() && (*c - 0.5).abs() < 1e-9));
    assert!(matches!(results.iter().find(|(a, _)| a == "a1").unwrap().1, StatsResult::NoSession), "ok:false means no live session");
    assert_eq!(stats_requests(&daemon).len(), 10, "no retries, no duplicate");
    // Once answered, the same agent may be asked again.
    client.request("e1", OutboundRequest::SessionStats { agent_id: "a0".into() }, "").unwrap();
    wait_until(|| stats_requests(&daemon).len() == 11).await;
}

fn list_requests(daemon: &FakeDaemon) -> Vec<Value> {
    daemon.received().into_iter().filter(|f| inner_type(f) == "list_agents_request").collect()
}

#[tokio::test]
async fn a_list_request_after_the_sync_is_answered_as_an_agents_listed_event() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = live(&daemon).await;
    client.request("e1", OutboundRequest::ListAgents, "").unwrap();
    wait_until(|| list_requests(&daemon).len() == 2).await; // the connect-time sync sent the first
    let id = list_requests(&daemon)[1]["message"]["requestId"].clone();
    daemon.push(session(json!({"type": "list_agents_response", "requestId": id, "agents": [
        {"agentId": "a1", "status": "idle", "cwd": "/work/app", "provider": "pi", "labels": {}},
        "junk",
    ]})));
    let events = collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::AgentsListed { .. })).await;
    let DaemonEventKind::AgentsListed { agents } = &events.last().unwrap().kind else { panic!("{:?}", events.last()) };
    assert_eq!((agents.len(), agents[0].agent_id.as_str(), agents[0].cwd.as_deref()), (1, "a1", Some("/work/app")));
    drop(client);
}

#[tokio::test]
async fn five_hundred_stats_requests_never_exceed_four_in_flight() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, _rx) = live(&daemon).await;
    for i in 0..500 {
        client.request("e1", OutboundRequest::SessionStats { agent_id: format!("a{i}") }, "").unwrap();
    }
    let mut answered = 0;
    let mut max_unanswered = 0;
    while answered < 500 {
        wait_until(|| stats_requests(&daemon).len() > answered).await;
        let sent = stats_requests(&daemon);
        max_unanswered = max_unanswered.max(sent.len() - answered);
        for req in sent.iter().skip(answered) {
            daemon.push(stats_response(&req["message"]["requestId"], true));
        }
        answered = sent.len();
    }
    assert!(max_unanswered <= 4, "{max_unanswered} in flight");
}

fn page(request_id: &Value, items: usize, seq_end: u64, has_newer: bool, end_cursor: &str) -> String {
    let items: Vec<Value> = (0..items).map(|i| json!({"kind": "other", "sourceSeq": i, "event": {"kind": "turn_started"}})).collect();
    session(json!({"type": "fetch_agent_timeline_response", "requestId": request_id, "agentId": "a1", "items": items,
        "seqStart": 0, "seqEnd": seq_end, "hasNewer": has_newer, "endCursor": end_cursor, "startCursor": "0"}))
}

fn timeline_requests(daemon: &FakeDaemon) -> Vec<Value> {
    daemon.received().into_iter().filter(|f| inner_type(f) == "fetch_agent_timeline_request").collect()
}

#[tokio::test]
async fn timeline_paging_follows_has_newer_with_cursors_until_the_last_page() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = live(&daemon).await;
    client.request("e1", OutboundRequest::FetchTimeline { agent_id: "a1".into() }, "").unwrap();
    wait_until(|| timeline_requests(&daemon).len() == 1).await;
    let first = timeline_requests(&daemon)[0]["message"].clone();
    assert_eq!((first["direction"].as_str(), first["limit"].as_u64()), (Some("before"), Some(200)));
    assert!(first.get("cursor").is_none(), "the newest page has no cursor");
    daemon.push(page(&first["requestId"], 3, 10, true, "10"));
    wait_until(|| timeline_requests(&daemon).len() == 2).await;
    let second = timeline_requests(&daemon)[1]["message"].clone();
    assert_eq!((second["direction"].as_str(), second["cursor"].as_str(), second["limit"].as_u64()), (Some("after"), Some("10"), Some(200)));
    daemon.push(page(&second["requestId"], 2, 20, true, "20"));
    wait_until(|| timeline_requests(&daemon).len() == 3).await;
    let third = timeline_requests(&daemon)[2]["message"].clone();
    assert_eq!(third["cursor"], "20");
    daemon.push(page(&third["requestId"], 1, 25, false, "25"));

    let events = support::collect_count(&mut rx, 3, |e| matches!(e.kind, DaemonEventKind::Timeline { .. })).await;
    let pages: Vec<(usize, bool, u64)> = events
        .iter()
        .filter_map(|e| match &e.kind {
            DaemonEventKind::Timeline { items, reset, seq_end, agent_id } => {
                assert_eq!(agent_id, "a1");
                Some((items.len(), *reset, *seq_end))
            }
            _ => None,
        })
        .collect();
    assert_eq!(pages, [(3, true, 10), (2, false, 20), (1, false, 25)]);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(timeline_requests(&daemon).len(), 3, "paging stops at !hasNewer");
}

#[tokio::test]
async fn timeline_paging_stops_at_two_thousand_items() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = live(&daemon).await;
    client.request("e1", OutboundRequest::FetchTimeline { agent_id: "a1".into() }, "").unwrap();
    for n in 1..=2 {
        wait_until(|| timeline_requests(&daemon).len() == n).await;
        let req = timeline_requests(&daemon)[n - 1]["message"].clone();
        daemon.push(page(&req["requestId"], 1000, 1000 * n as u64, true, &format!("{n}")));
    }
    let events = support::collect_count(&mut rx, 2, |e| matches!(e.kind, DaemonEventKind::Timeline { .. })).await;
    assert_eq!(events.iter().filter(|e| matches!(e.kind, DaemonEventKind::Timeline { .. })).count(), 2);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(timeline_requests(&daemon).len(), 2, "2,000 items fetched: no third page although hasNewer");
}

#[tokio::test]
async fn a_newer_timeline_load_cancels_the_one_in_progress() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = live(&daemon).await;
    client.request("e1", OutboundRequest::FetchTimeline { agent_id: "a1".into() }, "").unwrap();
    client.request("e1", OutboundRequest::FetchTimeline { agent_id: "a2".into() }, "").unwrap();
    wait_until(|| timeline_requests(&daemon).len() == 2).await;
    let reqs = timeline_requests(&daemon);
    // The superseded load's page arrives (with hasNewer) and must be ignored without paging on.
    daemon.push(page(&reqs[0]["message"]["requestId"], 4, 9, true, "9"));
    daemon.push(page(&reqs[1]["message"]["requestId"], 2, 7, false, "7"));
    let events = collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::Timeline { .. })).await;
    let DaemonEventKind::Timeline { agent_id, items, reset, .. } = &events.last().unwrap().kind else { panic!() };
    assert_eq!((agent_id.as_str(), items.len(), *reset), ("a2", 2, true));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(timeline_requests(&daemon).len(), 2, "no `after` page for the cancelled load");
    assert!(rx.try_recv().map_or(true, |rest| rest.iter().all(|e| !matches!(e.kind, DaemonEventKind::Timeline { .. }))));
}

#[tokio::test]
async fn stats_requests_while_not_live_fail_and_unknown_timeline_kinds_are_counted() {
    let daemon = FakeDaemon::start(Behavior::default()).await;
    let (client, mut rx) = start_with(1024);
    assert!(client.request("e1", OutboundRequest::SessionStats { agent_id: "a".into() }, "").is_err());
    client.connect(entry("e1", daemon.port()), None).unwrap();
    collect_until(&mut rx, is_snapshot).await;
    client.request("e1", OutboundRequest::FetchTimeline { agent_id: "a1".into() }, "").unwrap();
    wait_until(|| timeline_requests(&daemon).len() == 1).await;
    let id = timeline_requests(&daemon)[0]["message"]["requestId"].clone();
    daemon.push(session(json!({"type": "fetch_agent_timeline_response", "requestId": id, "items": [
        {"kind": "other", "sourceSeq": 1, "event": {"kind": "hologram"}},
        {"kind": "other", "sourceSeq": 2, "event": {"kind": "turn_started"}}],
        "seqStart": 1, "seqEnd": 2, "hasNewer": false})));
    collect_until(&mut rx, |e| matches!(e.kind, DaemonEventKind::Timeline { .. })).await;
    let diag = collect_until(&mut rx, |e| diagnostics(e).is_some_and(|c| c.unknown_enum_values == 1)).await;
    assert!(diagnostics(diag.last().unwrap()).is_some());
}
