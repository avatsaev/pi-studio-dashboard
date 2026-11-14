//! `MonitorState`: the one in-memory source of truth, and its invariants.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::agent::AgentEntry;
use crate::caps::{
    ASSISTANT_TAIL, DIALOG_MESSAGE, DIALOG_OPTION, DIALOG_OPTIONS, DIALOG_TITLE, DRILL_HOLD, DRILL_ITEMS, ERROR_TEXT, EVENT_RING, LABEL, OUTPUT_LINE,
    SUMMARY_TITLE, TOOL_OUTPUT_LINE, TOOL_OUTPUT_TAIL_LINES,
};
use crate::daemon::DaemonSlice;
use crate::dialog::UiRequestItem;
use crate::drill::{DrillBuffer, DrillItem};
use crate::settings::SettingsMirror;

const KEY_SEP: char = '\u{1f}';

/// Map key of an agent: `"<serverId>\u{1f}<agentId>"` (JSON-safe, O(log n) in a `BTreeMap`).
#[must_use]
pub fn agent_key(server_id: &str, agent_id: &str) -> String {
    format!("{server_id}{KEY_SEP}{agent_id}")
}

/// Map key of a dialog: `"<serverId>\u{1f}<requestId>"`.
#[must_use]
pub fn ui_request_key(server_id: &str, request_id: &str) -> String {
    format!("{server_id}{KEY_SEP}{request_id}")
}

/// Placeholder owned by the observability sprint (sprint-010).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Observability {}

/// Why a state violates its bounds or invariants.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Invalid {
    #[error("{field} is {bytes} bytes; the limit is {max}")]
    TextTooLong { field: &'static str, bytes: usize, max: usize },
    #[error("{field} holds {count} entries; the limit is {max}")]
    TooMany { field: &'static str, count: usize, max: usize },
    #[error("ring {field} holds {len} items; its capacity is {cap}")]
    RingOverCapacity { field: &'static str, len: usize, cap: usize },
    #[error("daemon entry id {0:?} appears more than once")]
    DuplicateEntryId(String),
    #[error("{map} key {key:?} does not match its entry")]
    KeyMismatch { map: &'static str, key: String },
}

/// Live-only dashboard state (nothing persisted, D8).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MonitorState {
    /// Config order, unique `entry_id`.
    pub daemons: Vec<DaemonSlice>,
    /// Keyed by [`agent_key`].
    pub agents: BTreeMap<String, AgentEntry>,
    /// Keyed by [`ui_request_key`].
    pub ui_requests: BTreeMap<String, UiRequestItem>,
    pub drilldown: Option<DrillBuffer>,
    pub observability: Observability,
    /// Events dropped by the reducers (unknown entry, or a snapshot that cannot be keyed).
    pub dropped_events: u64,
    pub settings: SettingsMirror,
}

impl MonitorState {
    #[must_use]
    pub fn daemon(&self, entry_id: &str) -> Option<&DaemonSlice> {
        self.daemons.iter().find(|d| d.entry_id == entry_id)
    }

    #[must_use]
    pub fn agent(&self, server_id: &str, agent_id: &str) -> Option<&AgentEntry> {
        self.agents.get(&agent_key(server_id, agent_id))
    }

    #[must_use]
    pub fn ui_request(&self, server_id: &str, request_id: &str) -> Option<&UiRequestItem> {
        self.ui_requests.get(&ui_request_key(server_id, request_id))
    }

    /// Insert or replace an agent under its own key.
    pub fn upsert_agent(&mut self, agent: AgentEntry) -> Option<AgentEntry> {
        self.agents.insert(agent_key(&agent.server_id, &agent.agent_id), agent)
    }

    /// Insert or replace a dialog under its own key.
    pub fn upsert_ui_request(&mut self, item: UiRequestItem) -> Option<UiRequestItem> {
        self.ui_requests.insert(ui_request_key(&item.server_id, &item.request_id), item)
    }

    /// Check every bound and invariant.
    ///
    /// # Errors
    /// The first violated bound or invariant, as an [`Invalid`] variant.
    pub fn validate(&self) -> Result<(), Invalid> {
        let mut seen = BTreeSet::new();
        for d in &self.daemons {
            if !seen.insert(d.entry_id.as_str()) {
                return Err(Invalid::DuplicateEntryId(d.entry_id.clone()));
            }
            text("daemon.name", &d.name, LABEL)?;
            ring_len("daemon.events", d.events.len(), EVENT_RING)?;
            for e in &d.events {
                text("event.summary", &e.summary, SUMMARY_TITLE)?;
            }
            if let Some(err) = &d.last_error {
                opt_text("daemon.lastError", err.message.as_deref(), ERROR_TEXT)?;
            }
        }
        for (key, a) in &self.agents {
            if *key != agent_key(&a.server_id, &a.agent_id) {
                return Err(Invalid::KeyMismatch { map: "agents", key: key.clone() });
            }
            validate_agent(a)?;
        }
        for (key, u) in &self.ui_requests {
            if *key != ui_request_key(&u.server_id, &u.request_id) {
                return Err(Invalid::KeyMismatch { map: "uiRequests", key: key.clone() });
            }
            validate_ui_request(u)?;
        }
        if let Some(buf) = &self.drilldown {
            validate_drill(buf)?;
        }
        Ok(())
    }
}

fn text(field: &'static str, s: &str, max: usize) -> Result<(), Invalid> {
    if s.len() > max { Err(Invalid::TextTooLong { field, bytes: s.len(), max }) } else { Ok(()) }
}

fn opt_text(field: &'static str, s: Option<&str>, max: usize) -> Result<(), Invalid> {
    s.map_or(Ok(()), |s| text(field, s, max))
}

fn ring_len(field: &'static str, len: usize, cap: usize) -> Result<(), Invalid> {
    if len > cap { Err(Invalid::RingOverCapacity { field, len, cap }) } else { Ok(()) }
}

fn validate_agent(a: &AgentEntry) -> Result<(), Invalid> {
    opt_text("agent.title", a.title.as_deref(), SUMMARY_TITLE)?;
    for l in &a.labels {
        text("agent.label", l, LABEL)?;
    }
    if let Some(t) = &a.activity.current_tool {
        text("agent.currentTool.label", &t.label, LABEL)?;
    }
    opt_text("agent.lastOutputLine", a.activity.last_output_line.as_deref(), OUTPUT_LINE)?;
    opt_text("agent.lastErrorMessage", a.activity.last_error_message.as_deref(), ERROR_TEXT)?;
    text("agent.assistantTail", &a.assistant_tail, ASSISTANT_TAIL)
}

fn validate_ui_request(u: &UiRequestItem) -> Result<(), Invalid> {
    opt_text("dialog.title", u.title.as_deref(), DIALOG_TITLE)?;
    opt_text("dialog.message", u.message.as_deref(), DIALOG_MESSAGE)?;
    opt_text("dialog.inFlightResponse", u.in_flight_response.as_deref(), DIALOG_OPTION)?;
    if let Some(opts) = &u.options {
        if opts.len() > DIALOG_OPTIONS {
            return Err(Invalid::TooMany { field: "dialog.options", count: opts.len(), max: DIALOG_OPTIONS });
        }
        for o in opts {
            text("dialog.option", o, DIALOG_OPTION)?;
        }
    }
    Ok(())
}

fn validate_drill(buf: &DrillBuffer) -> Result<(), Invalid> {
    ring_len("drilldown.items", buf.items.len(), DRILL_ITEMS)?;
    ring_len("drilldown.held", buf.held.len(), DRILL_HOLD)?;
    for item in &buf.items {
        if let DrillItem::Tool { label, output_tail: Some(lines), .. } = item {
            text("drill.tool.label", label, LABEL)?;
            if lines.len() > TOOL_OUTPUT_TAIL_LINES {
                return Err(Invalid::TooMany { field: "drill.tool.outputTail", count: lines.len(), max: TOOL_OUTPUT_TAIL_LINES });
            }
            for l in lines {
                text("drill.tool.outputLine", l, TOOL_OUTPUT_LINE)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::agent::{AgentStatus, UsageSnapshot};
    use crate::daemon::{EventKind, EventRecord};
    use crate::drill::{DrillState, DrillTarget, TurnKind};
    use crate::settings::Theme;

    fn populated() -> MonitorState {
        let mut s = MonitorState::default();
        let mut d = DaemonSlice::new("e1", "box");
        d.server_id = Some("srv".into());
        d.events.push(EventRecord { at: 1, kind: EventKind::DialogRaised, agent_id: Some("a1".into()), summary: "dialog raised".into() });
        s.daemons.push(d);
        s.upsert_agent(AgentEntry {
            server_id: "srv".into(),
            agent_id: "a1".into(),
            status: AgentStatus::Running,
            usage: UsageSnapshot::Some { tokens: None, cost: Some(0.5), context_usage: None, taken_at: 9 },
            ..AgentEntry::default()
        });
        s.upsert_ui_request(UiRequestItem {
            server_id: "srv".into(),
            request_id: "r1".into(),
            agent_id: "a1".into(),
            options: Some(vec!["x".into()]),
            ..UiRequestItem::default()
        });
        let mut buf = DrillBuffer { target: DrillTarget { server_id: "srv".into(), agent_id: "a1".into() }, state: DrillState::Live, ..DrillBuffer::default() };
        buf.items.push(DrillItem::Turn { kind: TurnKind::Started });
        s.drilldown = Some(buf);
        s.settings.theme = Theme::Dark;
        s
    }

    #[test]
    fn default_round_trips_and_validates() {
        let s = MonitorState::default();
        let back: MonitorState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, back);
        assert!(back.validate().is_ok());
    }

    #[test]
    fn populated_round_trips_with_camel_case_and_json_safe_keys() {
        let s = populated();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"uiRequests\"") && json.contains("\"connEpoch\"") && json.contains("\"closeToTray\""));
        assert!(json.contains("\"takenAt\":9") && json.contains("\"type\":\"turn\""));
        let back: MonitorState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
        assert!(back.validate().is_ok());
        assert!(back.agent("srv", "a1").is_some() && back.ui_request("srv", "r1").is_some());
    }

    #[test]
    fn drill_ring_holds_newest_2000() {
        let mut buf = DrillBuffer::default();
        for i in 0..5000 {
            buf.items.push(DrillItem::System { text: i.to_string() });
        }
        assert_eq!(buf.items.len(), 2000);
        assert_eq!(buf.items.last(), Some(&DrillItem::System { text: "4999".into() }));
    }

    #[test]
    fn hostile_ring_input_is_truncated() {
        let mut v = serde_json::to_value(DaemonSlice::new("e", "n")).unwrap();
        let events: Vec<_> = (0..700).map(|i| serde_json::json!({"at": i, "kind": "connState", "summary": "s"})).collect();
        v["events"] = events.into();
        let d: DaemonSlice = serde_json::from_value(v).unwrap();
        assert_eq!(d.events.len(), 500);
        assert_eq!(d.events.iter().next().map(|e| e.at), Some(200));
    }

    #[test]
    fn unknown_status_and_sparse_agent_are_lenient() {
        assert_eq!(AgentStatus::from_wire("hibernating"), AgentStatus::Unknown);
        assert_eq!(AgentStatus::from_wire("running"), AgentStatus::Running);
        let a: AgentEntry = serde_json::from_str(r#"{"serverId":"s","agentId":"a","status":"hibernating"}"#).unwrap();
        assert_eq!(a.status, AgentStatus::Unknown);
        let a: AgentEntry = serde_json::from_str("{}").unwrap();
        assert_eq!(a.usage, UsageSnapshot::None);
    }

    #[test]
    fn validate_flags_violations() {
        let mut s = populated();
        s.daemons.push(DaemonSlice::new("e1", "dup"));
        assert_eq!(s.validate(), Err(Invalid::DuplicateEntryId("e1".into())));

        let mut s = populated();
        for u in s.ui_requests.values_mut() {
            u.title = Some("t".repeat(DIALOG_TITLE + 1));
        }
        assert!(matches!(s.validate(), Err(Invalid::TextTooLong { field: "dialog.title", .. })));

        let mut s = populated();
        for _ in 0..=EVENT_RING {
            s.daemons[0].events.items.push_back(EventRecord { at: 0, kind: EventKind::ConnState, agent_id: None, summary: String::new() });
        }
        assert!(matches!(s.validate(), Err(Invalid::RingOverCapacity { field: "daemon.events", .. })));

        let mut s = populated();
        if let Some(u) = s.ui_requests.values_mut().next() {
            u.options = Some(vec!["o".into(); DIALOG_OPTIONS + 1]);
        }
        assert!(matches!(s.validate(), Err(Invalid::TooMany { field: "dialog.options", .. })));

        let mut s = populated();
        if let Some(a) = s.agents.remove(&agent_key("srv", "a1")) {
            s.agents.insert("wrong".into(), a);
        }
        assert!(matches!(s.validate(), Err(Invalid::KeyMismatch { map: "agents", .. })));
        assert!(s.validate().unwrap_err().to_string().contains("agents"));
    }
}
