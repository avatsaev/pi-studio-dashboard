#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use common::{item, run, snapshot};
use pistudiodashboard_ingest::{Filters, card_order, grouped_order};
use pistudiodashboard_model::{AgentListItem, ConnState, MonitorState};
use proptest::prelude::*;

const CWDS: [&str; 7] = ["/w/a/app", "/w/b/app", "/w/a/app/", "/", "  ", "C:\\x\\app", "/w/é"];

fn agents() -> impl Strategy<Value = Vec<(u8, usize, u8, Option<usize>)>> {
    // (agent number, daemon 0..2, status 0..4, cwd index or none)
    proptest::collection::vec((0..12u8, 0..2usize, 0..4u8, proptest::option::of(0..CWDS.len())), 0..24)
}

fn build(entries: &[(u8, usize, u8, Option<usize>)]) -> MonitorState {
    let mut s = common::state(&["e0", "e1"]);
    for (i, e) in ["e0", "e1"].iter().enumerate() {
        run(&mut s, &[common::hello(e, 1, &format!("s{i}")), common::conn(e, 1, ConnState::Live)], 1000);
        let list: Vec<AgentListItem> = entries
            .iter()
            .filter(|(_, d, ..)| *d == i)
            .map(|(n, _, st, cwd)| {
                let mut it = item(&format!("a{n}"), ["idle", "running", "error", "closed"][usize::from(*st)]);
                it.cwd = cwd.map(|c| CWDS[c].to_owned());
                it
            })
            .collect();
        run(&mut s, &[snapshot(e, 1, list, vec![])], 1000);
    }
    s
}

proptest! {
    /// Groups are deterministic whatever order agents arrive in, hold every scoped card exactly once,
    /// and keep the `card_order` sort inside each group.
    #[test]
    fn grouping_is_total_complete_and_order_independent(entries in agents()) {
        let mut reversed = entries.clone();
        reversed.reverse();
        let (a, b) = (build(&entries), build(&reversed));
        let f = Filters::default();
        // A later duplicate agent id replaces an earlier one, so reversing can change which cwd wins:
        // compare only when ids are unique.
        let ids: BTreeSet<_> = entries.iter().map(|(n, d, ..)| (*n, *d)).collect();
        if ids.len() == entries.len() {
            prop_assert_eq!(grouped_order(&a, &f), grouped_order(&b, &f));
        }
        let groups = grouped_order(&a, &f);
        let mut all: Vec<_> = groups.iter().flat_map(|g| g.cards.clone()).collect();
        prop_assert_eq!(all.len(), card_order(&a, &f).len());
        all.sort();
        all.dedup();
        prop_assert_eq!(all.len(), card_order(&a, &f).len());
        let flat = card_order(&a, &f);
        for g in &groups {
            prop_assert!(!g.cards.is_empty());
            let expected: Vec<_> = flat.iter().filter(|c| g.cards.contains(c)).cloned().collect();
            prop_assert_eq!(&g.cards, &expected);
        }
    }
}

#[test]
fn streaming_deltas_do_not_move_groups() {
    use common::{live_state, stream};
    use pistudiodashboard_model::StreamEvent;
    let mut s = live_state(&[("a1", "running"), ("a2", "idle")]);
    let before = grouped_order(&s, &Filters::default());
    run(&mut s, &[stream("a2", Some(1), StreamEvent::TurnStarted)], 2000);
    run(&mut s, &[stream("a1", Some(2), StreamEvent::TurnStarted)], 2001);
    assert_eq!(
        grouped_order(&s, &Filters::default()).iter().map(|g| g.cards.clone()).collect::<Vec<_>>(),
        before.iter().map(|g| g.cards.clone()).collect::<Vec<_>>()
    );
}
