//! `feed.tick`: the periodic stats trigger (D16, D39). The engine is the only scheduler.
//!
//! The app's owner loop calls it every 10 s while [`feed.tick`'s `needed`](needed) is true (some
//! live daemon has a running agent). It changes no state: it asks `stats_tick` which running
//! agents of live daemons need a refresh and executes the resulting `FetchStats` effects, which
//! the client deduplicates and rate-limits.

use pistudiodashboard_ingest::{needs_stats_tick, stats_tick};
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always};
use crate::{Result, Session, params};

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "feed.tick",
        label: "Refresh Usage",
        menu: &[],
        shortcut: None,
        internal: true,
        params: r#"{"now": n /*epoch ms, optional*/}"#,
        enabled: always,
        run: tick,
    }]
}

impl Session {
    /// Whether the owner loop needs to call `feed.tick` at all: a live daemon has a running agent.
    #[must_use]
    pub fn needs_tick(&self) -> bool {
        needs_stats_tick(&self.snapshot())
    }
}

fn tick(s: &mut Session, p: &Value) -> Result<Value> {
    let now = i64::try_from(params::u64_param(p, "now")?.unwrap_or(0)).unwrap_or(i64::MAX);
    let effects = stats_tick(&s.snapshot(), now);
    let requested = effects.len();
    let skipped = s.execute_effects(effects);
    let skipped: Vec<Value> = skipped.iter().map(|k| json!({"effect": k.effect, "reason": k.reason})).collect();
    Ok(json!({"requested": requested, "skipped": skipped, "needed": s.needs_tick()}))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use pistudiodashboard_model::{AgentEntry, AgentStatus, ConnState, DaemonSlice, MonitorState};
    use pistudiodashboard_protocol::OutboundRequest;

    use super::*;
    use crate::{EngineError, EngineServices};

    fn session(conn: ConnState) -> (Session, Rc<RefCell<Vec<OutboundRequest>>>) {
        let mut m = MonitorState::default();
        let mut d = DaemonSlice::new("e1", "box");
        d.server_id = Some("srv".into());
        d.conn = conn;
        m.daemons.push(d);
        for (id, st) in [("r1", AgentStatus::Running), ("r2", AgentStatus::Running), ("i1", AgentStatus::Idle)] {
            m.upsert_agent(AgentEntry { server_id: "srv".into(), agent_id: id.into(), status: st, ..AgentEntry::default() });
        }
        let log: Rc<RefCell<Vec<OutboundRequest>>> = Rc::default();
        let sink = Rc::clone(&log);
        let hook: crate::services::SendToDaemon = Box::new(move |_, req| {
            sink.borrow_mut().push(req);
            Ok(())
        });
        (Session::with_monitor(m).with_services(EngineServices { send_to_daemon: Some(hook), ..EngineServices::default() }), log)
    }

    #[test]
    fn a_tick_requests_stats_for_exactly_the_running_agents_of_a_live_daemon() {
        let (mut s, log) = session(ConnState::Live);
        assert!(s.needs_tick());
        let r = s.execute("feed.tick", json!({"now": 1000})).unwrap();
        assert_eq!((r["requested"].as_u64(), r["needed"].as_bool()), (Some(2), Some(true)));
        assert_eq!(*log.borrow(), [OutboundRequest::SessionStats { agent_id: "r1".into() }, OutboundRequest::SessionStats { agent_id: "r2".into() }]);
        assert_eq!(s.revision(), 0, "a tick changes no state");
    }

    #[test]
    fn a_disconnected_daemon_is_not_polled_and_needs_no_tick() {
        let (mut s, log) = session(ConnState::Backoff);
        assert!(!s.needs_tick());
        let r = s.execute("feed.tick", json!({})).unwrap();
        assert_eq!((r["requested"].as_u64(), r["needed"].as_bool()), (Some(0), Some(false)));
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn bad_params_are_errors_and_a_missing_hook_is_skipped() {
        let (mut s, _) = session(ConnState::Live);
        assert!(matches!(s.execute("feed.tick", json!({"now": "x"})), Err(EngineError::BadParams(_))));
        let mut bare = Session::with_monitor((*s.snapshot()).clone());
        let r = bare.execute("feed.tick", json!({})).unwrap();
        assert_eq!(r["skipped"].as_array().unwrap().len(), 2);
    }
}
