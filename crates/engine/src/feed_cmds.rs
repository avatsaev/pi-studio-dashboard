//! `feed.ingest`: the one way live data enters the session, and the effect executor.
//!
//! The batch is folded by the ingest reducers on a private copy (one `update`, so one revision
//! bump per batch); effects are handed to the executor only after the new state is published.
//! A missing or failing hook never fails the command: the effect is reported in `skipped`.

use pistudiodashboard_ingest::apply_in_place;
use pistudiodashboard_model::{DaemonEvent, Effect, NotifyKind, ToastKind};
use pistudiodashboard_protocol::OutboundRequest;
use serde::Deserialize as _;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always};
use crate::services::{Hook, Notification, NotifyAction, NotifyTarget};
use crate::session::Toast;
use crate::{EngineError, Result, Session};

/// An effect that could not be executed, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub effect: &'static str,
    pub reason: String,
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "feed.ingest",
        label: "Ingest Daemon Events",
        menu: &[],
        shortcut: None,
        internal: true,
        params: r#"{"events": DaemonEvent[]}"#,
        enabled: always,
        run: ingest,
    }]
}

fn effect_name(e: &Effect) -> &'static str {
    match e {
        Effect::Notify { .. } => "notify",
        Effect::ResolveNotification { .. } => "resolveNotification",
        Effect::Toast { .. } => "toast",
        Effect::CloseDrill { .. } => "closeDrill",
        Effect::FetchStats { .. } => "fetchStats",
        Effect::FetchAgents { .. } => "fetchAgents",
        Effect::FetchTimeline { .. } => "fetchTimeline",
    }
}

fn ingest(s: &mut Session, p: &Value) -> Result<Value> {
    let raw = p.get("events").ok_or_else(|| EngineError::BadParams("`events` is required".into()))?;
    let events = Vec::<DaemonEvent>::deserialize(raw).map_err(|e| EngineError::BadParams(format!("`events`: {e}")))?;
    let mut effects = Vec::new();
    s.update(|monitor| {
        for event in &events {
            apply_in_place(monitor, event, event.at, &mut effects);
        }
        Ok(())
    })?;
    s.note_events(&events);
    let names: Vec<&str> = effects.iter().map(effect_name).collect();
    let names = json!(names);
    let skipped = s.execute_effects(effects);
    let skipped: Vec<Value> = skipped.iter().map(|k| json!({"effect": k.effect, "reason": k.reason})).collect();
    Ok(json!({"applied": events.len(), "effects": names, "skipped": skipped}))
}

impl Session {
    /// Execute effects on the owner thread, after the state that produced them is published.
    pub fn execute_effects(&mut self, effects: Vec<Effect>) -> Vec<Skipped> {
        let mut skipped = Vec::new();
        for effect in effects {
            let name = effect_name(&effect);
            if let Err(reason) = self.execute_effect(effect) {
                skipped.push(Skipped { effect: name, reason });
            }
        }
        skipped
    }

    fn execute_effect(&mut self, effect: Effect) -> std::result::Result<(), String> {
        match effect {
            Effect::Toast { kind, text } => {
                self.pending_toasts.push(Toast { kind, text });
                Ok(())
            }
            Effect::CloseDrill { reason } => self.close_drill(reason),
            Effect::FetchStats { server_id, agent_id } => self.send(&server_id, OutboundRequest::SessionStats { agent_id }),
            Effect::FetchAgents { server_id } => self.send(&server_id, OutboundRequest::ListAgents),
            Effect::FetchTimeline { server_id, agent_id } => self.send(&server_id, OutboundRequest::FetchTimeline { agent_id }),
            Effect::Notify { kind, server_id, item_id } => self.notify(NotifyAction::Raise, Some(kind), server_id, item_id),
            Effect::ResolveNotification { server_id, item_id } => self.notify(NotifyAction::Resolve, None, server_id, item_id),
        }
    }

    fn close_drill(&mut self, reason: String) -> std::result::Result<(), String> {
        if self.snapshot().drilldown.is_some() {
            self.update(|m| {
                m.drilldown = None;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        }
        self.pending_toasts.push(Toast { kind: ToastKind::Info, text: reason });
        Ok(())
    }

    fn send(&mut self, server_id: &str, request: OutboundRequest) -> std::result::Result<(), String> {
        self.services.require(Hook::SendToDaemon)?;
        let Some(hook) = self.services.send_to_daemon.as_mut() else { return Err("no live connection in this build".into()) };
        hook(server_id, request).map_err(|e| e.to_string())
    }

    fn notify(&mut self, action: NotifyAction, kind: Option<NotifyKind>, server_id: String, item_id: String) -> std::result::Result<(), String> {
        self.services.require(Hook::Notify)?;
        let (title, body) = self.notification_text(kind, &server_id, &item_id);
        let Some(hook) = self.services.notify.as_mut() else { return Err("notifications are not available in this build".into()) };
        hook(Notification { action, title, body, target: NotifyTarget { server_id, item_id } });
        Ok(())
    }

    /// Fixed vocabulary only: the agent's title (or "Agent") and the daemon's name.
    fn notification_text(&self, kind: Option<NotifyKind>, server_id: &str, item_id: &str) -> (String, String) {
        let Some(kind) = kind else { return (String::new(), String::new()) };
        let monitor = self.snapshot();
        let agent_id = match item_id.split_once(':') {
            Some(("ui", request)) => monitor.ui_request(server_id, request).map(|u| u.agent_id.clone()),
            Some(("fail", agent)) => Some(agent.to_owned()),
            _ => None,
        };
        let title = agent_id.and_then(|a| monitor.agent(server_id, &a).and_then(|e| e.title.clone())).unwrap_or_else(|| "Agent".to_owned());
        let daemon =
            monitor.daemons.iter().find(|d| d.server_id.as_deref() == Some(server_id) && d.duplicate_of.is_none()).map_or("daemon", |d| d.name.as_str());
        let what = match kind {
            NotifyKind::NeedsInput => "needs input",
            NotifyKind::Failures => "failed",
        };
        (title, format!("{daemon}: {what}"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use pistudiodashboard_model::{DaemonSlice, DrillBuffer, DrillTarget, MonitorState};

    use super::*;
    use crate::{EngineServices, SendError};

    fn session_with_daemon() -> Session {
        let mut m = MonitorState::default();
        m.daemons.push(DaemonSlice::new("e1", "box"));
        Session::with_monitor(m)
    }

    fn hello_and_snapshot() -> Value {
        json!({"events": [
            {"entryId": "e1", "serverId": "srv", "connEpoch": 1, "seq": 0, "at": 10,
             "kind": {"type": "hello", "info": {}, "caps": {}, "degraded": [], "tlsInUse": false, "isLoopback": true}},
            {"entryId": "e1", "connEpoch": 1, "seq": 1, "at": 11,
             "kind": {"type": "snapshot", "agents": [{"agentId": "a1", "status": "running"}], "uiPending": []}}
        ]})
    }

    type Log = Rc<RefCell<Vec<(String, OutboundRequest)>>>;

    fn recording(result: std::result::Result<(), SendError>) -> (Session, Log) {
        let log: Log = Rc::default();
        let sink = Rc::clone(&log);
        let services = EngineServices {
            send_to_daemon: Some(Box::new(move |sid, req| {
                sink.borrow_mut().push((sid.to_owned(), req));
                result
            })),
            ..EngineServices::default()
        };
        (session_with_daemon().with_services(services), log)
    }

    #[test]
    fn a_batch_bumps_the_revision_once_and_effects_run_after_publishing() {
        let (mut s, log) = recording(Ok(()));
        let held = s.snapshot();
        let r = s.execute("feed.ingest", hello_and_snapshot()).unwrap();
        assert_eq!(s.revision(), 1);
        assert_eq!(r["applied"], 2);
        assert_eq!(r["effects"], json!(["fetchStats"]));
        assert_eq!(r["skipped"], json!([]));
        assert!(held.agents.is_empty() && s.snapshot().agents.len() == 1, "readers keep the old state");
        assert_eq!(*log.borrow(), [("srv".to_owned(), OutboundRequest::SessionStats { agent_id: "a1".into() })]);
    }

    #[test]
    fn an_agent_created_after_the_snapshot_asks_the_daemon_for_its_workspace() {
        let (mut s, log) = recording(Ok(()));
        s.execute("feed.ingest", hello_and_snapshot()).unwrap();
        log.borrow_mut().clear();
        let new = json!({"events": [{"entryId": "e1", "connEpoch": 1, "seq": 2, "at": 20,
            "kind": {"type": "agentUpdate", "agentId": "new", "status": "initializing"}}]});
        let r = s.execute("feed.ingest", new).unwrap();
        assert_eq!(r["effects"], json!(["fetchStats", "fetchAgents"]));
        assert_eq!(
            *log.borrow(),
            [("srv".to_owned(), OutboundRequest::SessionStats { agent_id: "new".into() }), ("srv".to_owned(), OutboundRequest::ListAgents)]
        );
        let listed = json!({"events": [{"entryId": "e1", "connEpoch": 1, "seq": 3, "at": 21,
            "kind": {"type": "agentsListed", "agents": [{"agentId": "new", "status": "idle", "cwd": "/work/app/"}]}}]});
        s.execute("feed.ingest", listed).unwrap();
        let v = s.execute("document.inspect", json!({"groupBy": "workspace"})).unwrap();
        let labels: Vec<&str> = v["groups"]["window"].as_array().unwrap().iter().map(|g| g["label"].as_str().unwrap()).collect();
        assert!(labels.contains(&"app") && labels.contains(&"No workspace"), "{labels:?}: `new` joins `app`, `a1` (never given a cwd) stays out");
        assert_eq!(v["cards"]["window"].as_array().unwrap().iter().find(|c| c["agentId"] == "new").unwrap()["cwd"], "/work/app");
    }

    #[test]
    fn missing_hook_and_send_errors_skip_without_failing() {
        let mut s = session_with_daemon();
        let r = s.execute("feed.ingest", hello_and_snapshot()).unwrap();
        assert_eq!(r["skipped"][0]["effect"], "fetchStats");
        assert_eq!(r["skipped"][0]["reason"], "no live connection in this build");
        let (mut s, _) = recording(Err(SendError::NotConnected));
        let r = s.execute("feed.ingest", hello_and_snapshot()).unwrap();
        assert_eq!(r["skipped"][0]["reason"], "not connected");
        assert_eq!(s.snapshot().agents.len(), 1, "the state change stands");
    }

    #[test]
    fn a_malformed_batch_is_rejected_with_state_untouched() {
        let mut s = session_with_daemon();
        let before = s.snapshot();
        for bad in [
            json!({"events": "x"}),
            json!({}),
            json!({"events": [{"entryId": 5}]}),
            json!({"events": [{"entryId": "e1", "connEpoch": 1, "seq": 0, "at": 1, "kind": {"type": "nope"}}]}),
        ] {
            assert!(matches!(s.execute("feed.ingest", bad), Err(EngineError::BadParams(_))));
        }
        assert!(matches!(s.execute("feed.ingest", json!([])), Err(EngineError::BadParams(_))));
        assert!(Arc::ptr_eq(&before, &s.snapshot()));
        assert_eq!(s.revision(), 0);
    }

    #[test]
    fn close_drill_clears_the_buffer_and_queues_a_toast() {
        let mut s = session_with_daemon();
        s.update(|m| {
            m.drilldown = Some(DrillBuffer { target: DrillTarget { server_id: "srv".into(), agent_id: "a".into() }, ..DrillBuffer::default() });
            Ok(())
        })
        .unwrap();
        assert!(s.execute_effects(vec![Effect::CloseDrill { reason: "agent gone".into() }]).is_empty());
        assert!(s.snapshot().drilldown.is_none());
        assert_eq!(s.take_toasts(), [Toast { kind: ToastKind::Info, text: "agent gone".into() }]);
        assert!(s.take_toasts().is_empty());
    }

    #[test]
    fn notify_uses_fixed_vocabulary() {
        let seen: Rc<RefCell<Vec<Notification>>> = Rc::default();
        let sink = Rc::clone(&seen);
        let mut s =
            session_with_daemon().with_services(EngineServices { notify: Some(Box::new(move |n| sink.borrow_mut().push(n))), ..EngineServices::default() });
        s.execute("feed.ingest", hello_and_snapshot()).unwrap();
        let skipped = s.execute_effects(vec![Effect::Notify { kind: NotifyKind::Failures, server_id: "srv".into(), item_id: "fail:a1".into() }]);
        assert!(skipped.is_empty());
        let n = &seen.borrow()[0];
        assert_eq!((n.title.as_str(), n.body.as_str()), ("Agent", "box: failed"));
        assert_eq!(n.target.item_id, "fail:a1");
    }
}
