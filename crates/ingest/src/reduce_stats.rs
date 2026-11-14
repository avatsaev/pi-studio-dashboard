//! Stats reducer and the periodic stats trigger (D16, D39).

use pistudiodashboard_model::{AgentStatus, ConnState, Effect, MonitorState, StatsResult, UsageSnapshot, agent_key};

use crate::apply::Ctx;
use crate::keys::server_iter;
use crate::reduce_agent::owner;

/// `Stats`: `Usage` replaces the card's usage (taken at the event time), `NoSession` clears it.
/// An unknown agent is ignored.
pub(crate) fn stats(state: &mut MonitorState, ctx: Ctx, agent_id: &str, result: &StatsResult) {
    let Some(sid) = owner(state, ctx) else { return };
    let Some(entry) = state.agents.get_mut(&agent_key(&sid, agent_id)) else { return };
    entry.usage = match result {
        StatsResult::NoSession => UsageSnapshot::None,
        StatsResult::Usage { tokens, cost, context_usage } => {
            UsageSnapshot::Some { tokens: tokens.clone(), cost: *cost, context_usage: context_usage.clone(), taken_at: ctx.at }
        }
    };
}

/// The periodic trigger: a `FetchStats` for every running, non-archived agent of every live,
/// non-duplicate daemon. Pure; the owner loop calls it every 10 s while something runs (D39).
#[must_use]
pub fn stats_tick(state: &MonitorState, _now: i64) -> Vec<Effect> {
    let mut out = Vec::new();
    for d in &state.daemons {
        let (ConnState::Live, Some(sid), None) = (d.conn, d.server_id.as_deref(), &d.duplicate_of) else { continue };
        for (_, a) in server_iter(&state.agents, sid) {
            if a.status == AgentStatus::Running && !a.archived {
                out.push(Effect::FetchStats { server_id: sid.to_owned(), agent_id: a.agent_id.clone() });
            }
        }
    }
    out
}

/// Whether any live daemon has a running agent: the owner loop uses it to decide whether the
/// tick needs to fire at all.
#[must_use]
pub fn needs_stats_tick(state: &MonitorState) -> bool {
    !stats_tick(state, 0).is_empty()
}
