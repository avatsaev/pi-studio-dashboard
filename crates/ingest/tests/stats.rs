#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{conn, hello, item, kind, live_state, run, snapshot, state};
use pistudiodashboard_ingest::{Filters, needs_stats_tick, stats_tick, usage_totals};
use pistudiodashboard_model::{ConnState, ContextUsage, DaemonEventKind, Effect, StatsResult, TokenUsage, UsageSnapshot};

fn stats_event(agent: &str, result: StatsResult, at: i64) -> pistudiodashboard_model::DaemonEvent {
    let mut e = kind(DaemonEventKind::Stats { agent_id: agent.into(), result });
    e.at = at;
    e
}

fn usage(tokens: Option<TokenUsage>, cost: Option<f64>) -> StatsResult {
    StatsResult::Usage { tokens, cost, context_usage: None }
}

#[test]
fn stats_tick_asks_only_for_running_agents_of_live_daemons() {
    let mut s = live_state(&[("a1", "running"), ("a2", "idle"), ("a3", "running"), ("a4", "closed")]);
    let fx = stats_tick(&s, 0);
    assert_eq!(
        fx,
        [Effect::FetchStats { server_id: "srv".into(), agent_id: "a1".into() }, Effect::FetchStats { server_id: "srv".into(), agent_id: "a3".into() }]
    );
    assert!(needs_stats_tick(&s));
    run(&mut s, &[kind(DaemonEventKind::AgentArchived { agent_id: "a3".into(), archived_at: None })], 1);
    assert_eq!(stats_tick(&s, 0).len(), 1, "archived agents are not polled");
    run(&mut s, &[conn("e1", 2, ConnState::Backoff)], 2);
    assert!(stats_tick(&s, 0).is_empty() && !needs_stats_tick(&s), "a disconnected daemon is not polled");
}

#[test]
fn stats_tick_skips_duplicate_daemons_and_idle_only_states() {
    let mut s = state(&["e1", "e2"]);
    run(&mut s, &[hello("e1", 1, "srv"), conn("e1", 1, ConnState::Live), snapshot("e1", 1, vec![item("a1", "running")], vec![])], 1);
    run(&mut s, &[hello("e2", 1, "srv"), conn("e2", 1, ConnState::Live)], 2);
    assert!(s.daemons[1].duplicate_of.is_some());
    assert_eq!(stats_tick(&s, 0).len(), 1, "the same agents are not polled twice");
    let idle = live_state(&[("a1", "idle")]);
    assert!(stats_tick(&idle, 0).is_empty());
}

#[test]
fn the_stats_reducer_stores_usage_at_the_event_time_and_no_session_clears_it() {
    let mut s = live_state(&[("a1", "running")]);
    let ctx = ContextUsage { tokens: Some(10), window: Some(1000), percent: Some(1.0) };
    let tokens = TokenUsage { input: Some(100), output: Some(50), total: None };
    run(&mut s, &[stats_event("a1", StatsResult::Usage { tokens: Some(tokens.clone()), cost: Some(0.25), context_usage: Some(ctx.clone()) }, 777)], 1);
    assert_eq!(s.agent("srv", "a1").unwrap().usage, UsageSnapshot::Some { tokens: Some(tokens), cost: Some(0.25), context_usage: Some(ctx), taken_at: 777 });
    run(&mut s, &[stats_event("a1", StatsResult::NoSession, 800)], 2);
    assert_eq!(s.agent("srv", "a1").unwrap().usage, UsageSnapshot::None);
    run(&mut s, &[stats_event("ghost", usage(None, None), 1)], 3);
    assert!(s.agent("srv", "ghost").is_none());
}

#[test]
fn a_snapshot_keeps_the_usage_and_its_as_of_time() {
    let mut s = live_state(&[("a1", "idle")]);
    run(&mut s, &[stats_event("a1", usage(Some(TokenUsage { total: Some(5), ..TokenUsage::default() }), None), 321)], 1);
    let fx = run(&mut s, &[conn("e1", 2, ConnState::Live), snapshot("e1", 2, vec![item("a1", "idle")], vec![])], 2);
    assert!(matches!(s.agent("srv", "a1").unwrap().usage, UsageSnapshot::Some { taken_at: 321, .. }));
    assert!(fx.is_empty(), "idle agent with usage needs no fetch on reconnect");
}

#[test]
fn totals_use_total_else_input_plus_output_and_label_partial_coverage() {
    let mut s = live_state(&[("a1", "idle"), ("a2", "idle"), ("a3", "idle"), ("a4", "idle"), ("a5", "idle")]);
    run(
        &mut s,
        &[
            stats_event("a1", usage(Some(TokenUsage { total: Some(1200), ..TokenUsage::default() }), None), 1),
            stats_event("a2", usage(Some(TokenUsage { input: Some(100), output: Some(50), total: None }), Some(0.5)), 1),
            stats_event("a3", usage(None, Some(0.25)), 1),
            stats_event("a4", StatsResult::NoSession, 1),
        ],
        1,
    );
    let t = usage_totals(&s, &Filters::default());
    assert_eq!((t.tokens, t.covered, t.of), (1350, 3, 5));
    assert!((t.cost - 0.75).abs() < 1e-9);
    assert_eq!(t.label(), "3 of 5 agents");
    let one = usage_totals(&s, &Filters { statuses: vec![pistudiodashboard_model::AgentStatus::Closed], ..Filters::default() });
    assert_eq!((one.covered, one.of, one.label().as_str()), (0, 0, "0 of 0 agents"));
}
