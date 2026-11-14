//! Drill-down selection: `agent.select` / `agent.deselect`.
//!
//! Selection lives in `MonitorState.drilldown.target` (not in UI state). `agent.select` opens an
//! empty `loading` buffer and requests the newest timeline page; the client does the paging and
//! the reducers fill the buffer from the resulting `Timeline` events (D39).

use pistudiodashboard_ingest::{DrillError, drill_deselect, drill_select};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session, params};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "agent.select",
            label: "Open Agent Details",
            menu: &[],
            shortcut: None,
            internal: false,
            params: r#"{"serverId": string, "agentId": string}"#,
            enabled: crate::commands::always,
            run: select,
        },
        CommandSpec {
            id: "agent.deselect",
            label: "Close Agent Details",
            menu: &["Agent"],
            shortcut: Some("Escape"),
            internal: false,
            params: "{}",
            enabled: deselect_enabled,
            run: deselect,
        },
    ]
}

fn deselect_enabled(s: &Session) -> std::result::Result<(), String> {
    if s.snapshot().drilldown.is_some() { Ok(()) } else { Err("no agent details are open".into()) }
}

fn select(s: &mut Session, p: &Value) -> Result<Value> {
    let server_id = params::req_str(p, "serverId")?;
    let agent_id = params::req_str(p, "agentId")?;
    let effects = s.update(|m| {
        drill_select(m, server_id, agent_id)
            .map_err(|DrillError::NotAvailable| EngineError::Disabled { id: "agent.select".into(), reason: "agent not available".into() })
    })?;
    let skipped = s.execute_effects(effects);
    Ok(
        json!({"serverId": server_id, "agentId": agent_id, "skipped": skipped.iter().map(|k| json!({"effect": k.effect, "reason": k.reason})).collect::<Vec<_>>()}),
    )
}

fn deselect(s: &mut Session, _: &Value) -> Result<Value> {
    s.update(|m| {
        drill_deselect(m);
        Ok(())
    })?;
    Ok(json!({}))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use pistudiodashboard_model::{AgentEntry, AgentStatus, DaemonSlice, MonitorState};
    use pistudiodashboard_protocol::OutboundRequest;

    use super::*;
    use crate::EngineServices;

    type Log = Rc<RefCell<Vec<(String, OutboundRequest)>>>;

    fn session() -> (Session, Log) {
        let mut m = MonitorState::default();
        let mut d = DaemonSlice::new("e1", "box");
        d.server_id = Some("srv".into());
        m.daemons.push(d);
        m.upsert_agent(AgentEntry { server_id: "srv".into(), agent_id: "a1".into(), status: AgentStatus::Running, ..AgentEntry::default() });
        m.upsert_agent(AgentEntry { server_id: "srv".into(), agent_id: "a2".into(), ..AgentEntry::default() });
        m.upsert_agent(AgentEntry { server_id: "srv".into(), agent_id: "gone".into(), archived: true, ..AgentEntry::default() });
        let log: Log = Rc::default();
        let sink = Rc::clone(&log);
        let hook: crate::services::SendToDaemon = Box::new(move |sid, req| {
            sink.borrow_mut().push((sid.to_owned(), req));
            Ok(())
        });
        (Session::with_monitor(m).with_services(EngineServices { send_to_daemon: Some(hook), ..EngineServices::default() }), log)
    }

    #[test]
    fn select_opens_a_loading_buffer_and_requests_the_newest_page() {
        let (mut s, log) = session();
        s.execute("agent.select", json!({"serverId": "srv", "agentId": "a1"})).unwrap();
        let d = s.snapshot().drilldown.clone().unwrap();
        assert_eq!((d.target.agent_id.as_str(), d.state, d.items.len()), ("a1", pistudiodashboard_model::DrillState::Loading, 0));
        assert_eq!(*log.borrow(), [("srv".to_owned(), OutboundRequest::FetchTimeline { agent_id: "a1".into() })]);
        assert_eq!(s.revision(), 1);
    }

    #[test]
    fn selecting_another_agent_replaces_the_buffer_immediately() {
        let (mut s, log) = session();
        s.execute("agent.select", json!({"serverId": "srv", "agentId": "a1"})).unwrap();
        s.execute("agent.select", json!({"serverId": "srv", "agentId": "a2"})).unwrap();
        assert_eq!(s.snapshot().drilldown.as_ref().unwrap().target.agent_id, "a2");
        assert_eq!(log.borrow().len(), 2);
    }

    #[test]
    fn unknown_and_archived_agents_are_not_available_and_change_nothing() {
        let (mut s, log) = session();
        for id in ["nope", "gone"] {
            let e = s.execute("agent.select", json!({"serverId": "srv", "agentId": id})).unwrap_err();
            assert_eq!(e, EngineError::Disabled { id: "agent.select".into(), reason: "agent not available".into() });
        }
        assert!(s.snapshot().drilldown.is_none() && log.borrow().is_empty());
        assert_eq!(s.revision(), 0);
        for bad in [json!({}), json!({"serverId": "srv"}), json!({"serverId": 1, "agentId": "a1"})] {
            assert!(matches!(s.execute("agent.select", bad), Err(EngineError::BadParams(_))));
        }
    }

    #[test]
    fn deselect_needs_an_open_drill_and_frees_the_buffer() {
        let (mut s, _) = session();
        let e = s.execute("agent.deselect", json!({})).unwrap_err();
        assert!(matches!(e, EngineError::Disabled { reason, .. } if reason == "no agent details are open"));
        s.execute("agent.select", json!({"serverId": "srv", "agentId": "a1"})).unwrap();
        s.execute("agent.deselect", json!({})).unwrap();
        assert!(s.snapshot().drilldown.is_none());
    }

    #[test]
    fn the_drill_loads_from_ingested_json_events_end_to_end() {
        let (mut s, _) = session();
        s.execute("agent.select", json!({"serverId": "srv", "agentId": "a1"})).unwrap();
        let ev = |seq: u64, kind: Value| json!({"entryId": "e1", "serverId": "srv", "connEpoch": 1, "seq": seq, "at": 10, "kind": kind});
        s.execute(
            "feed.ingest",
            json!({"events": [
                ev(1, json!({"type": "agentStream", "agentId": "a1", "seq": 2, "event": {"type": "assistantMessage", "text": "held "}})),
                ev(2, json!({"type": "timeline", "agentId": "a1", "reset": true, "seqEnd": 1,
                    "items": [{"kind": "other", "sourceSeq": 0, "event": {"type": "userMessage", "text": "hi"}}]})),
                ev(3, json!({"type": "agentStream", "agentId": "a1", "seq": 3, "event": {"type": "assistantMessage", "text": "live"}})),
                ev(4, json!({"type": "agentUpdate", "agentId": "a1", "status": "idle", "title": null})),
            ]}),
        )
        .unwrap();
        let m = s.snapshot();
        let d = m.drilldown.as_ref().unwrap();
        assert_eq!((d.state, d.seq_high, d.items.len()), (pistudiodashboard_model::DrillState::Live, Some(3), 2));
        let a = m.agent("srv", "a1").unwrap();
        assert_eq!((a.status, a.title.clone()), (AgentStatus::Idle, None));
        assert_eq!(a.activity.last_output_line.as_deref(), Some("held live"));
    }

    #[test]
    fn a_missing_send_hook_does_not_fail_the_selection() {
        let (mut s, _) = session();
        let mut bare = Session::with_monitor((*s.snapshot()).clone());
        let r = bare.execute("agent.select", json!({"serverId": "srv", "agentId": "a1"})).unwrap();
        assert_eq!(r["skipped"][0]["effect"], "fetchTimeline");
        assert!(bare.snapshot().drilldown.is_some());
        let _ = &mut s;
    }
}
