#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{conn, ev, hello, item, pending, run, snapshot, state};
use pistudiodashboard_ingest::{Filters, apply, apply_batch, card_order, daemon_status_counts, usage_totals};
use pistudiodashboard_model::{AgentStatus, ConnState, DaemonEventKind, DrillBuffer, DrillState, DrillTarget, Effect, TokenUsage, UsageSnapshot, agent_key};

fn live(s: &mut pistudiodashboard_model::MonitorState, entry: &str, sid: &str) {
    run(s, &[hello(entry, 1, sid), conn(entry, 1, ConnState::Live)], 10);
}

#[test]
fn stale_epoch_is_dropped_and_newer_conn_state_raises_it() {
    let mut s = state(&["e1"]);
    run(&mut s, &[conn("e1", 3, ConnState::Live)], 10);
    assert_eq!(s.daemons[0].conn_epoch, 3);
    let before = s.clone();
    let (after, fx) = apply(&s, &ev("e1", 2, DaemonEventKind::Health { reachable: true, at: 1 }), 11);
    assert_eq!(before, after);
    assert!(fx.is_empty());
    let (after, _) = apply(&s, &ev("e1", 3, DaemonEventKind::Health { reachable: true, at: 1 }), 11);
    assert!(after.daemons[0].health.is_some());
    run(&mut s, &[conn("e1", 4, ConnState::Probing)], 12);
    assert_eq!(s.daemons[0].conn_epoch, 4);
}

#[test]
fn disconnect_marks_stale_and_snapshot_replaces_in_one_step() {
    let mut s = state(&["e1"]);
    live(&mut s, "e1", "srv");
    run(&mut s, &[snapshot("e1", 1, vec![item("a", "idle"), item("b", "running")], vec![pending("r1", "a")])], 20);
    assert_eq!(s.agent("srv", "a").unwrap().attention.pending_dialogs, 1);
    run(&mut s, &[conn("e1", 1, ConnState::Backoff)], 30);
    assert_eq!(s.daemons[0].stale_since, Some(30));
    assert!(s.agents.values().all(|a| a.stale));
    assert!(s.ui_requests.values().all(|u| u.stale));
    assert_eq!(card_order(&s, &Filters::default()).len(), 2);

    let (next, _) = apply_batch(&s, &[snapshot("e1", 2, vec![item("a", "idle")], vec![])], 40);
    assert_eq!(next.daemons[0].stale_since, None);
    assert_eq!(next.agents.len(), 1);
    assert!(next.agents.values().all(|a| !a.stale));
    assert!(next.ui_requests.is_empty(), "omitted dialog is gone");
}

#[test]
fn dialogs_upsert_by_key_and_filter_non_items() {
    let mut s = state(&["e1"]);
    live(&mut s, "e1", "srv");
    let mut no_resp = pending("r2", "a");
    no_resp.expects_response = false;
    let mut removed = pending("r3", "a");
    removed.removed = true;
    run(&mut s, &[snapshot("e1", 1, vec![item("a", "idle")], vec![pending("r1", "a"), pending("r1", "a"), no_resp, removed])], 20);
    assert_eq!(s.ui_requests.len(), 1);
    assert!(s.ui_request("srv", "r1").is_some());
}

#[test]
fn duplicate_server_id_counts_once_and_changed_id_removes_old() {
    let mut s = state(&["e1", "e2"]);
    run(&mut s, &[hello("e1", 1, "srv"), hello("e2", 1, "srv")], 10);
    assert_eq!(s.daemons[1].duplicate_of.as_deref(), Some("e1"));
    run(&mut s, &[snapshot("e1", 1, vec![item("a", "idle")], vec![]), snapshot("e2", 1, vec![item("a", "idle")], vec![])], 20);
    assert_eq!(s.agents.len(), 1);
    assert_eq!(s.dropped_events, 1, "duplicate snapshot dropped and counted");
    assert_eq!(card_order(&s, &Filters::default()).len(), 1);
    assert!(daemon_status_counts(&s, "e2").is_empty());
    // a duplicate disconnect must not stale the owner's agents
    run(&mut s, &[conn("e2", 1, ConnState::Backoff)], 25);
    assert!(s.agents.values().all(|a| !a.stale));

    s.drilldown = Some(DrillBuffer { target: DrillTarget { server_id: "srv".into(), agent_id: "a".into() }, ..DrillBuffer::default() });
    let fx = run(&mut s, &[hello("e1", 2, "other")], 30);
    assert!(s.agents.is_empty());
    assert!(s.drilldown.is_none());
    assert!(fx.contains(&Effect::CloseDrill { reason: "daemon replaced".into() }));
    assert_eq!(s.daemons[0].server_id.as_deref(), Some("other"));
}

#[test]
fn snapshot_refreshes_drill_target() {
    let mut s = state(&["e1"]);
    live(&mut s, "e1", "srv");
    s.drilldown =
        Some(DrillBuffer { target: DrillTarget { server_id: "srv".into(), agent_id: "a".into() }, state: DrillState::Live, ..DrillBuffer::default() });
    let fx = run(&mut s, &[snapshot("e1", 1, vec![item("a", "idle")], vec![])], 20);
    assert_eq!(s.drilldown.as_ref().unwrap().state, DrillState::Refetching);
    assert_eq!(fx.iter().filter(|e| matches!(e, Effect::FetchTimeline { .. })).count(), 1);
    let fx = run(&mut s, &[snapshot("e1", 1, vec![item("z", "idle")], vec![])], 21);
    assert!(s.drilldown.is_none());
    assert!(fx.contains(&Effect::CloseDrill { reason: "agent gone".into() }));
}

#[test]
fn snapshot_fetches_stats_only_for_running_or_without_usage() {
    let mut s = state(&["e1"]);
    live(&mut s, "e1", "srv");
    run(&mut s, &[snapshot("e1", 1, vec![item("a", "idle"), item("b", "idle")], vec![])], 20);
    if let Some(a) = s.agents.get_mut(&agent_key("srv", "a")) {
        a.usage = UsageSnapshot::Some { tokens: None, cost: None, context_usage: None, taken_at: 7 };
    }
    let fx = run(&mut s, &[snapshot("e1", 2, vec![item("a", "idle"), item("b", "idle"), item("c", "running")], vec![])], 30);
    let ids: Vec<_> = fx.iter().filter_map(|e| if let Effect::FetchStats { agent_id, .. } = e { Some(agent_id.as_str()) } else { None }).collect();
    assert_eq!(ids, ["b", "c"]);
    assert_eq!(s.agent("srv", "a").unwrap().usage, UsageSnapshot::Some { tokens: None, cost: None, context_usage: None, taken_at: 7 }, "usage carried over");
}

#[test]
fn card_order_ranks_with_tie_breaks_and_excludes_archived() {
    let mut s = state(&["e1"]);
    live(&mut s, "e1", "srv");
    let list = vec![
        item("closed", "closed"),
        item("idle-b", "idle"),
        item("idle-a", "idle"),
        item("err", "error"),
        item("run", "running"),
        item("dlg", "idle"),
        item("arch", "idle"),
    ];
    run(&mut s, &[snapshot("e1", 1, list, vec![pending("r", "dlg")])], 20);
    if let Some(a) = s.agents.get_mut(&agent_key("srv", "arch")) {
        a.archived = true;
    }
    // a snapshot marks an `error` agent as errored (needs attention); acknowledge it to see rank 2
    let errored = s.agent("srv", "err").unwrap().attention.errored;
    assert!(errored);
    if let Some(a) = s.agents.get_mut(&agent_key("srv", "err")) {
        a.attention.errored = false;
    }
    if let Some(a) = s.agents.get_mut(&agent_key("srv", "idle-b")) {
        a.last_activity = Some(50);
    }
    let order: Vec<_> = card_order(&s, &Filters::default()).into_iter().map(|(_, a)| a).collect();
    assert_eq!(order, ["dlg", "run", "err", "idle-b", "idle-a", "closed"]);
    let f = Filters { statuses: vec![AgentStatus::Idle], ..Filters::default() };
    assert_eq!(card_order(&s, &f).len(), 3);
    assert_eq!(daemon_status_counts(&s, "e1").get(&AgentStatus::Idle), Some(&3));
}

#[test]
fn usage_totals_cover_three_of_five() {
    let mut s = state(&["e1"]);
    live(&mut s, "e1", "srv");
    let list: Vec<_> = ["a", "b", "c", "d", "e"].iter().map(|i| item(i, "idle")).collect();
    run(&mut s, &[snapshot("e1", 1, list, vec![])], 20);
    let set = |s: &mut pistudiodashboard_model::MonitorState, id: &str, tokens: TokenUsage, cost: f64| {
        if let Some(a) = s.agents.get_mut(&agent_key("srv", id)) {
            a.usage = UsageSnapshot::Some { tokens: Some(tokens), cost: Some(cost), context_usage: None, taken_at: 1 };
        }
    };
    set(&mut s, "a", TokenUsage { total: Some(100), input: Some(1), output: Some(1) }, 0.5);
    set(&mut s, "b", TokenUsage { total: None, input: Some(10), output: Some(5) }, 0.25);
    set(&mut s, "c", TokenUsage::default(), 0.0);
    let t = usage_totals(&s, &Filters::default());
    assert_eq!((t.covered, t.of, t.tokens), (3, 5, 115));
    assert!((t.cost - 0.75).abs() < 1e-9);
}

#[test]
fn unknown_entry_is_dropped_counted_and_creates_nothing() {
    let s = state(&["e1"]);
    let (after, fx) = apply(&s, &conn("nope", 1, ConnState::Live), 1);
    assert_eq!(after.dropped_events, 1);
    assert!(fx.is_empty());
    assert_eq!(after.daemons, s.daemons);
}

#[test]
fn rtt_diagnostics_and_health_touch_counters_only() {
    let mut s = state(&["e1"]);
    let mut c = pistudiodashboard_model::CountersSnapshot { malformed: 2, ..Default::default() };
    c.unknown_types.insert("x".into(), 3);
    c.unknown_types.insert("y".into(), 4);
    run(&mut s, &[ev("e1", 0, DaemonEventKind::Rtt { ms: 9 }), ev("e1", 0, DaemonEventKind::Diagnostics(c))], 1);
    let k = &s.daemons[0].counters;
    assert_eq!((k.malformed, k.unknown), (2, 7));
    assert_eq!(k.unknown_types.iter().map(|u| (u.name.as_str(), u.count)).collect::<Vec<_>>(), [("y", 4), ("x", 3)], "most seen first");
    assert!(s.agents.is_empty());
}

#[test]
fn diagnostics_keep_only_the_ten_most_seen_unknown_types() {
    let mut s = state(&["e1"]);
    let mut c = pistudiodashboard_model::CountersSnapshot::default();
    for i in 0..12u64 {
        c.unknown_types.insert(format!("t{i:02}"), i + 1);
    }
    run(&mut s, &[ev("e1", 0, DaemonEventKind::Diagnostics(c))], 1);
    let k = &s.daemons[0].counters;
    assert_eq!(k.unknown, 78, "the sum covers every type");
    assert_eq!(k.unknown_types.len(), 10);
    assert_eq!((k.unknown_types[0].name.as_str(), k.unknown_types[0].count), ("t11", 12));
    assert_eq!(k.unknown_types[9].count, 3);
}
