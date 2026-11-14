//! View derivations: pure functions of `MonitorState`, never stored.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use pistudiodashboard_model::{AgentEntry, AgentStatus, MonitorState, UsageSnapshot};

use crate::keys::server_iter;

/// Card filters; an empty list means "all".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    /// Daemon config `entry_id`s.
    pub daemons: Vec<String>,
    pub statuses: Vec<AgentStatus>,
}

/// Sums over the filtered cards; `covered` of `of` agents have a usage snapshot (D16).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageTotals {
    pub tokens: u64,
    pub cost: f64,
    pub covered: usize,
    pub of: usize,
}

impl UsageTotals {
    /// "N of M agents": a partial sum is never presented as the whole.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{} of {} agents", self.covered, self.of)
    }
}

pub(crate) fn needs_attention(a: &AgentEntry) -> bool {
    a.attention.pending_dialogs > 0 || a.attention.errored
}

pub(crate) fn rank(a: &AgentEntry) -> u8 {
    if needs_attention(a) {
        return 0;
    }
    match a.status {
        AgentStatus::Running => 1,
        AgentStatus::Error => 2,
        AgentStatus::Idle | AgentStatus::Initializing | AgentStatus::Unknown => 3,
        AgentStatus::Closed => 4,
    }
}

/// Visible agents with their daemon's config position: not archived, daemon known and not a
/// duplicate, passing the daemon and status filters.
pub(crate) fn scope<'a>(state: &'a MonitorState, f: &Filters) -> Vec<(usize, &'a AgentEntry)> {
    let mut out = Vec::new();
    for (pos, d) in state.daemons.iter().enumerate() {
        let Some(sid) = d.server_id.as_deref() else { continue };
        if d.duplicate_of.is_some() || !(f.daemons.is_empty() || f.daemons.contains(&d.entry_id)) {
            continue;
        }
        for (_, a) in server_iter(&state.agents, sid) {
            if !a.archived && (f.statuses.is_empty() || f.statuses.contains(&a.status)) {
                out.push((pos, a));
            }
        }
    }
    out
}

pub(crate) type SortKey<'a> = (u8, usize, bool, Reverse<i64>, bool, &'a str, &'a str);

pub(crate) fn sort_key<'a>(pos: usize, a: &'a AgentEntry) -> SortKey<'a> {
    (rank(a), pos, a.last_activity.is_none(), Reverse(a.last_activity.unwrap_or(0)), a.title.is_none(), a.title.as_deref().unwrap_or(""), a.agent_id.as_str())
}

/// Card order as `(server_id, agent_id)`: needs-attention first, then running, error, idle-like,
/// closed; ties by daemon config order, `last_activity` desc (nulls last), title asc, agent id.
/// A total order over distinct cards, independent of event delivery order.
#[must_use]
pub fn card_order(state: &MonitorState, filters: &Filters) -> Vec<(String, String)> {
    let mut cards = scope(state, filters);
    cards.sort_by(|(pa, a), (pb, b)| sort_key(*pa, a).cmp(&sort_key(*pb, b)));
    cards.into_iter().map(|(_, a)| (a.server_id.clone(), a.agent_id.clone())).collect()
}

/// Token and cost sums over the filtered cards. Tokens = `total`, else `input + output`;
/// costs are summed without conversion.
#[must_use]
pub fn usage_totals(state: &MonitorState, filters: &Filters) -> UsageTotals {
    let mut t = UsageTotals::default();
    for (_, a) in scope(state, filters) {
        t.of += 1;
        if let UsageSnapshot::Some { tokens, cost, .. } = &a.usage {
            t.covered += 1;
            if let Some(k) = tokens {
                t.tokens = t.tokens.saturating_add(k.total.unwrap_or_else(|| k.input.unwrap_or(0).saturating_add(k.output.unwrap_or(0))));
            }
            t.cost += cost.unwrap_or(0.0);
        }
    }
    t
}

/// Non-archived agents of one daemon by status; empty for an unknown or duplicate entry.
#[must_use]
pub fn daemon_status_counts(state: &MonitorState, entry_id: &str) -> BTreeMap<AgentStatus, usize> {
    let mut counts = BTreeMap::new();
    let Some(d) = state.daemon(entry_id) else { return counts };
    let (Some(sid), None) = (d.server_id.as_deref(), &d.duplicate_of) else { return counts };
    for (_, a) in server_iter(&state.agents, sid).filter(|(_, a)| !a.archived) {
        *counts.entry(a.status).or_insert(0) += 1;
    }
    counts
}
