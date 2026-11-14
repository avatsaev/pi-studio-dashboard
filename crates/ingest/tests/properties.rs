#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use common::state;
use pistudiodashboard_ingest::{Filters, apply_batch, card_order};
use pistudiodashboard_model::{
    AgentListItem, ConnState, ContextUsage, CountersSnapshot, DaemonEvent, DaemonEventKind, DrillBuffer, DrillTarget, PendingUi, ServerCaps, ServerInfoView,
    StatsResult, StreamEvent, TimelineItem, TokenUsage, ToolInfo,
};
use proptest::prelude::*;

fn conn_state() -> impl Strategy<Value = ConnState> {
    prop_oneof![
        Just(ConnState::Idle),
        Just(ConnState::Live),
        Just(ConnState::Backoff),
        Just(ConnState::AuthFailed),
        Just(ConnState::Syncing),
        Just(ConnState::HostRejected),
    ]
}

fn list_item() -> impl Strategy<Value = AgentListItem> {
    (0..6u8, prop_oneof![Just("idle"), Just("running"), Just("error"), Just("closed"), Just("weird")], proptest::option::of(0..4i64)).prop_map(
        |(id, st, act)| AgentListItem {
            agent_id: format!("a{id}"),
            status: st.into(),
            title: Some(format!("t{}", id % 3)),
            last_activity: act,
            ..AgentListItem::default()
        },
    )
}

fn pending() -> impl Strategy<Value = PendingUi> {
    (0..4u8, 0..6u8, any::<bool>(), any::<bool>()).prop_map(|(r, a, e, rm)| PendingUi {
        request_id: format!("r{r}"),
        agent_id: format!("a{a}"),
        method: "confirm".into(),
        expects_response: e,
        removed: rm,
        title: Some("x".repeat(300)),
        options: Some(vec!["o".repeat(200); 60]),
        ..PendingUi::default()
    })
}

fn stream_event() -> impl Strategy<Value = StreamEvent> {
    let text = prop_oneof![Just(String::new()), Just("a\nb".to_owned()), Just("é".repeat(3000))];
    prop_oneof![
        text.clone().prop_map(|t| StreamEvent::AssistantMessage { text: t, is_final: false }),
        Just(StreamEvent::AssistantMessage { text: String::new(), is_final: true }),
        text.clone().prop_map(|t| StreamEvent::Reasoning { text: Some(t), is_final: false }),
        (0..3u8, prop_oneof![Just("started"), Just("running"), Just("completed"), Just("error"), Just("weird")], any::<bool>(), text.clone()).prop_map(
            |(c, status, partial, out)| StreamEvent::ToolCall {
                call_id: format!("c{c}"),
                status: status.into(),
                partial,
                tool: ToolInfo { kind: "shell".into(), detail: Some(out.clone()), output: Some(out) }
            }
        ),
        Just(StreamEvent::TurnStarted),
        Just(StreamEvent::TurnCompleted),
        text.clone().prop_map(|t| StreamEvent::TurnFailed { error: Some(t) }),
        Just(StreamEvent::TurnCanceled),
        text.prop_map(|t| StreamEvent::Error { message: Some(t) }),
        Just(StreamEvent::Compaction { phase: Some("started".into()), reason: None }),
        Just(StreamEvent::UserMessage { text: Some("hi".into()) }),
        Just(StreamEvent::Unknown { kind: "x".into() }),
    ]
}

fn agent_id() -> impl Strategy<Value = String> {
    (0..6u8).prop_map(|i| format!("a{i}"))
}

fn kind() -> impl Strategy<Value = DaemonEventKind> {
    prop_oneof![
        (
            agent_id(),
            proptest::option::of(prop_oneof![Just("idle"), Just("running"), Just("error"), Just("closed"), Just("weird")]),
            proptest::option::of(proptest::option::of(Just("é".repeat(400))))
        )
            .prop_map(|(agent_id, status, title)| DaemonEventKind::AgentUpdate {
                agent_id,
                status: status.map(Into::into),
                title,
                labels: Some(vec!["l".repeat(300); 3]),
                model: None,
                model_provider: None
            }),
        (agent_id(), prop_oneof![Just("idle"), Just("error")]).prop_map(|(agent_id, status)| DaemonEventKind::AgentStatus { agent_id, status: status.into() }),
        agent_id().prop_map(|agent_id| DaemonEventKind::AgentArchived { agent_id, archived_at: None }),
        agent_id().prop_map(|agent_id| DaemonEventKind::AgentDeleted { agent_id }),
        (agent_id(), proptest::option::of(0..40u64), stream_event()).prop_map(|(agent_id, seq, event)| DaemonEventKind::AgentStream {
            agent_id,
            seq,
            timestamp: None,
            event
        }),
        agent_id().prop_map(|agent_id| DaemonEventKind::TimelineReset { agent_id, reason: None }),
        (agent_id(), proptest::option::of(0..100u64), any::<bool>()).prop_map(|(agent_id, tokens, ok)| DaemonEventKind::Stats {
            agent_id,
            result: if ok {
                StatsResult::Usage {
                    tokens: tokens.map(|t| TokenUsage { total: Some(t), ..TokenUsage::default() }),
                    cost: Some(0.5),
                    context_usage: Some(ContextUsage::default()),
                }
            } else {
                StatsResult::NoSession
            }
        }),
        (agent_id(), proptest::collection::vec((0..50u64, stream_event()), 0..30), any::<bool>(), 0..60u64).prop_map(|(agent_id, items, reset, seq_end)| {
            DaemonEventKind::Timeline {
                agent_id,
                items: items.into_iter().map(|(source_seq, event)| TimelineItem { kind: "other".into(), source_seq, timestamp: None, event }).collect(),
                reset,
                seq_end,
            }
        }),
        (conn_state(), any::<i64>()).prop_map(|(state, since)| DaemonEventKind::ConnState { state, since, last_error: None, next_retry_in_ms: None }),
        Just(DaemonEventKind::Hello {
            info: ServerInfoView::default(),
            caps: ServerCaps::default(),
            degraded: BTreeSet::new(),
            tls_in_use: false,
            is_loopback: true
        }),
        any::<bool>().prop_map(|reachable| DaemonEventKind::Health { reachable, at: 1 }),
        (proptest::collection::vec(list_item(), 0..8), proptest::collection::vec(pending(), 0..5))
            .prop_map(|(agents, ui_pending)| DaemonEventKind::Snapshot { agents, ui_pending }),
        any::<u64>().prop_map(|ms| DaemonEventKind::Rtt { ms }),
        Just(DaemonEventKind::Diagnostics(CountersSnapshot::default())),
    ]
}

fn event() -> impl Strategy<Value = DaemonEvent> {
    (prop_oneof![Just("e1"), Just("e2"), Just("e3"), Just("ghost")], 0..5u64, prop_oneof![Just("s1"), Just("s2"), Just("s3")], any::<bool>(), kind()).prop_map(
        |(entry, epoch, sid, tagged, kind)| DaemonEvent {
            entry_id: entry.into(),
            server_id: tagged.then(|| sid.into()),
            conn_epoch: epoch,
            seq: 0,
            at: 1,
            kind,
        },
    )
}

proptest! {
    #[test]
    fn apply_is_total_deterministic_and_bounded(events in proptest::collection::vec(event(), 0..60), now in any::<i64>()) {
        let mut start = state(&["e1", "e2", "e3"]);
        start.drilldown = Some(DrillBuffer { target: DrillTarget { server_id: "s1".into(), agent_id: "a1".into() }, ..DrillBuffer::default() });
        let (a, fa) = apply_batch(&start, &events, now);
        let (b, fb) = apply_batch(&start, &events, now);
        prop_assert_eq!(&a, &b);
        prop_assert_eq!(fa, fb);
        prop_assert!(a.validate().is_ok(), "{:?}", a.validate());
        let order = card_order(&a, &Filters::default());
        let unique: BTreeSet<_> = order.iter().collect();
        prop_assert_eq!(unique.len(), order.len());
        // one batch equals event-by-event folding
        let mut folded = start.clone();
        for e in &events {
            folded = apply_batch(&folded, std::slice::from_ref(e), now).0;
        }
        prop_assert_eq!(a, folded);
    }
}
