//! The monitor view returned by `document.inspect`: built only from `MonitorState` and the
//! ingest derivations. Camel-case JSON; the frontend draws exactly this (agent-monitor § View
//! shape). The engine has no clock, so "taken ago" strings are the frontend's job
//! (`usage.takenAt` is epoch ms).

use std::collections::BTreeMap;

use pistudiodashboard_config::GroupBy;
use pistudiodashboard_ingest::{
    Filters, WorkspaceGroup, card_order, daemon_status_counts, grouped_order, label_of, usage_totals, workspace_labels, workspace_of,
};
use pistudiodashboard_model::{AgentEntry, AgentStatus, DrillBuffer, DrillState, MonitorState, UsageSnapshot};
use serde_json::{Map, Value, json};

use crate::{EngineError, Result, Session, params};

/// Workspace labels by normalised path, for the filtered scope.
type Labels = BTreeMap<String, String>;

/// Above this many cards the frontend virtualises the grid.
const VIRTUALISE_ABOVE: usize = 100;
/// Cards carried beyond each edge of the reported window, and the most one view carries.
const OVERSCAN: usize = 20;
const MAX_WINDOW: usize = 200;

/// Chip order and wire names of the status buckets.
const BUCKETS: [(AgentStatus, &str); 6] = [
    (AgentStatus::Running, "running"),
    (AgentStatus::Error, "error"),
    (AgentStatus::Idle, "idle"),
    (AgentStatus::Initializing, "initializing"),
    (AgentStatus::Closed, "closed"),
    (AgentStatus::Unknown, "unknown"),
];

struct ViewParams {
    daemons: Vec<String>,
    statuses: Vec<AgentStatus>,
    window: Option<(usize, usize)>,
    rail_collapsed: bool,
    group_by: GroupBy,
}

fn bad(msg: String) -> EngineError {
    EngineError::BadParams(msg)
}

fn string_list(p: &Value, key: &str) -> Result<Vec<String>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => {
            a.iter().map(|v| v.as_str().map(str::to_owned).ok_or_else(|| bad(format!("`{key}` must hold strings, got {}", params::kind(v))))).collect()
        }
        Some(other) => Err(bad(format!("`{key}` must be an array, got {}", params::kind(other)))),
    }
}

fn status_of(name: &str) -> Option<AgentStatus> {
    BUCKETS.iter().find(|(_, n)| *n == name).map(|(s, _)| *s)
}

fn status_name(s: AgentStatus) -> &'static str {
    BUCKETS.iter().find(|(b, _)| *b == s).map_or("unknown", |(_, n)| n)
}

fn window(p: &Value) -> Result<Option<(usize, usize)>> {
    let Some(w) = p.get("gridWindow").filter(|w| !w.is_null()) else { return Ok(None) };
    if !w.is_object() {
        return Err(bad(format!("`gridWindow` must be an object, got {}", params::kind(w))));
    }
    let start = params::req_u64(w, "start").map_err(|_| bad("`gridWindow.start` must be a non-negative integer".into()))?;
    let count = params::req_u64(w, "count").map_err(|_| bad("`gridWindow.count` must be a non-negative integer".into()))?;
    let clamp = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
    Ok(Some((clamp(start), clamp(count))))
}

fn group_by(p: &Value) -> Result<GroupBy> {
    match p.get("groupBy") {
        None | Some(Value::Null) => Ok(GroupBy::None),
        Some(Value::String(s)) => GroupBy::from_name(s).ok_or_else(|| bad(format!("`groupBy` is one of: none, workspace; got `{s}`"))),
        Some(other) => Err(bad(format!("`groupBy` must be a string (none, workspace), got {}", params::kind(other)))),
    }
}

fn read_params(p: &Value) -> Result<ViewParams> {
    let statuses = string_list(p, "filterStatuses")?
        .into_iter()
        .map(|n| status_of(&n).ok_or_else(|| bad(format!("unknown status `{n}` in `filterStatuses`"))))
        .collect::<Result<Vec<_>>>()?;
    Ok(ViewParams {
        daemons: string_list(p, "filterDaemons")?,
        statuses,
        window: window(p)?,
        rail_collapsed: params::bool_param(p, "railCollapsed")?.unwrap_or(false),
        group_by: group_by(p)?,
    })
}

fn usage(u: &UsageSnapshot) -> Value {
    match u {
        UsageSnapshot::None => json!("none"),
        UsageSnapshot::Some { tokens, cost, context_usage, taken_at } => {
            let total = tokens.as_ref().and_then(|t| t.total.or_else(|| Some(t.input.unwrap_or(0).saturating_add(t.output.unwrap_or(0)))));
            json!({
                "tokens": total,
                "cost": cost,
                "contextPercent": context_usage.as_ref().and_then(|c| c.percent),
                "takenAt": taken_at,
            })
        }
    }
}

struct Names<'a>(BTreeMap<&'a str, (&'a str, Option<i64>)>);

impl<'a> Names<'a> {
    fn new(m: &'a MonitorState) -> Self {
        let mut map = BTreeMap::new();
        for d in m.daemons.iter().filter(|d| d.duplicate_of.is_none()) {
            if let Some(sid) = d.server_id.as_deref() {
                map.entry(sid).or_insert((d.name.as_str(), d.stale_since));
            }
        }
        Self(map)
    }

    fn get(&self, server_id: &str) -> &str {
        self.0.get(server_id).map_or("", |(name, _)| name)
    }

    /// Epoch ms the agent's daemon went stale, if it is.
    fn stale_since(&self, server_id: &str) -> Option<i64> {
        self.0.get(server_id).and_then(|(_, since)| *since)
    }
}

fn card(m: &MonitorState, names: &Names<'_>, labels: &Labels, a: &AgentEntry) -> Value {
    let workspace = workspace_of(a.cwd.as_deref());
    let selected = m.drilldown.as_ref().is_some_and(|d| d.target.server_id == a.server_id && d.target.agent_id == a.agent_id);
    json!({
        "serverId": a.server_id,
        "agentId": a.agent_id,
        "daemonName": names.get(&a.server_id),
        "title": a.title.as_deref().unwrap_or(&a.agent_id),
        "model": a.model,
        "cwd": workspace,
        "workspaceLabel": label_of(labels, workspace.as_deref()),
        "status": status_name(a.status),
        "statusLabel": status_name(a.status),
        "dimmed": a.status == AgentStatus::Closed || a.stale,
        "stale": a.stale,
        "staleSince": if a.stale { names.stale_since(&a.server_id) } else { None },
        "attention": {"pendingDialogs": a.attention.pending_dialogs, "errored": a.attention.errored},
        "currentTool": a.activity.current_tool.as_ref().map(|t| json!({"label": t.label, "status": t.status})),
        "lastOutputLine": a.activity.last_output_line,
        "usage": usage(&a.usage),
        "selected": selected,
    })
}

fn drill(m: &MonitorState, names: &Names<'_>, labels: &Labels, d: &DrillBuffer) -> Value {
    let state = match d.state {
        DrillState::Loading => "loading",
        DrillState::Live => "live",
        DrillState::Refetching => "refetching",
    };
    json!({
        "key": {"serverId": d.target.server_id, "agentId": d.target.agent_id},
        "header": m.agent(&d.target.server_id, &d.target.agent_id).map(|a| card(m, names, labels, a)),
        "items": d.items.iter().map(|i| serde_json::to_value(i).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        "truncatedAtStart": d.truncated_at_start,
        "state": state,
        "stale": d.stale,
    })
}

/// An injective id for a group: length-prefixed daemon id, then the path.
fn group_key(g: &WorkspaceGroup) -> String {
    format!("{}:{}{}", g.server_id.len(), g.server_id, g.workspace.as_deref().unwrap_or(""))
}

/// The `groups` block: headers of the groups that touch the card slice `[start, end)`, and (when
/// the list is virtualised) every group's card count so the frontend can place any header.
fn groups_block(groups: &[WorkspaceGroup], names: &Names<'_>, start: usize, end: usize, virtualised: bool) -> Value {
    let mut first = 0usize;
    let mut window = Vec::new();
    for g in groups {
        let count = g.cards.len();
        if first < end && first + count > start {
            window.push(json!({
                "key": group_key(g),
                "serverId": g.server_id,
                "daemonName": names.get(&g.server_id),
                "label": g.label,
                "path": g.workspace,
                "count": count,
                "start": first,
                "needsAttention": g.needs_attention,
            }));
        }
        first += count;
    }
    let mut block = Map::new();
    block.insert("total".into(), json!(groups.len()));
    block.insert("window".into(), Value::Array(window));
    if virtualised {
        block.insert("layout".into(), json!(groups.iter().map(|g| g.cards.len()).collect::<Vec<_>>()));
    }
    Value::Object(block)
}

fn daemon_chips(s: &Session, m: &MonitorState, selected: &[String]) -> Vec<Value> {
    m.daemons
        .iter()
        .map(|d| {
            let count: usize = daemon_status_counts(m, &d.entry_id).values().sum();
            let (has_password, in_memory_only, keychain_unavailable) = s.password_flags(&d.entry_id);
            let entry = s.config().daemons.iter().find(|e| e.entry_id == d.entry_id);
            json!({
                "entryId": d.entry_id,
                "serverId": d.server_id,
                "name": d.name,
                "conn": d.conn.as_str(),
                "staleSince": d.stale_since,
                "selected": d.server_id.as_ref().is_some_and(|s| selected.contains(s)),
                "agentCount": count,
                "hasPassword": has_password,
                "passwordInMemoryOnly": in_memory_only,
                "keychainUnavailable": keychain_unavailable,
                "duplicateOf": d.duplicate_of,
                "lastError": d.last_error,
                "nextRetryInMs": d.next_retry_in_ms,
                "hostname": d.info.as_ref().and_then(|i| i.hostname.clone()),
                "version": d.info.as_ref().and_then(|i| i.version.clone()),
                "host": entry.map(|e| e.host.clone()),
                "port": entry.map(|e| e.port),
                "tls": entry.is_some_and(|e| e.tls),
                "webUiUrl": entry.and_then(|e| e.web_ui_url.clone()),
                "autoDetected": entry.is_some_and(|e| e.auto_detected),
                "unencrypted": crate::daemon_view::unencrypted(entry, has_password),
                "degraded": crate::daemon_view::chip_degraded(d),
            })
        })
        .collect()
}

fn status_chips(m: &MonitorState, daemons: &[String], selected: &[AgentStatus]) -> Vec<Value> {
    let ids = card_order(m, &Filters { daemons: daemons.to_vec(), statuses: Vec::new() });
    let mut counts: BTreeMap<AgentStatus, usize> = BTreeMap::new();
    for (sid, aid) in &ids {
        if let Some(a) = m.agent(sid, aid) {
            // An unknown status is counted and filtered with idle (the chips are the five known buckets).
            let bucket = if a.status == AgentStatus::Unknown { AgentStatus::Idle } else { a.status };
            *counts.entry(bucket).or_insert(0) += 1;
        }
    }
    BUCKETS
        .iter()
        .filter(|(status, _)| *status != AgentStatus::Unknown)
        .filter_map(|(status, name)| {
            let count = counts.get(status).copied().unwrap_or(0);
            let sel = selected.contains(status);
            (count > 0 || sel).then(|| json!({"bucket": name, "count": count, "selected": sel}))
        })
        .collect()
}

/// The statuses a status filter matches: selecting `idle` also matches `unknown`.
fn filter_statuses(selected: &[AgentStatus]) -> Vec<AgentStatus> {
    let mut out = selected.to_vec();
    if out.contains(&AgentStatus::Idle) && !out.contains(&AgentStatus::Unknown) {
        out.push(AgentStatus::Unknown);
    }
    out
}

/// Which slice of the ordered cards the view carries: all of them up to `VIRTUALISE_ABOVE`, else
/// the window the frontend reported, widened by `OVERSCAN` on both sides and never beyond
/// `MAX_WINDOW` cards. `None` (no window reported yet) is the first screenful.
fn card_range(total: usize, window: Option<(usize, usize)>) -> (usize, usize) {
    if total <= VIRTUALISE_ABOVE {
        return (0, total);
    }
    let (start, count) = window.unwrap_or((0, VIRTUALISE_ABOVE));
    let from = start.min(total).saturating_sub(OVERSCAN);
    let to = start.min(total).saturating_add(count).saturating_add(OVERSCAN).min(total);
    (from, to.min(from + MAX_WINDOW))
}

/// Build the `document.inspect` result.
pub(crate) fn monitor_view(s: &Session, p: &Value) -> Result<Value> {
    let vp = read_params(p)?;
    let m = s.snapshot();
    let selected_entries: Vec<String> =
        m.daemons.iter().filter(|d| d.server_id.as_ref().is_some_and(|sid| vp.daemons.contains(sid))).map(|d| d.entry_id.clone()).collect();
    // A filter naming no known daemon matches nothing (not "everything").
    let daemon_filter = if vp.daemons.is_empty() {
        Vec::new()
    } else if selected_entries.is_empty() {
        vec![String::new()]
    } else {
        selected_entries
    };
    let filters = Filters { daemons: daemon_filter.clone(), statuses: filter_statuses(&vp.statuses) };
    let names = Names::new(&m);
    let totals = usage_totals(&m, &filters);
    let (order, labels, groups) = if vp.group_by == GroupBy::Workspace {
        let groups = grouped_order(&m, &filters);
        let labels: Labels = groups.iter().filter_map(|g| g.workspace.clone().map(|w| (w, g.label.clone()))).collect();
        (groups.iter().flat_map(|g| g.cards.iter().cloned()).collect::<Vec<_>>(), labels, Some(groups))
    } else {
        (card_order(&m, &filters), workspace_labels(&m, &filters), None)
    };
    let total = order.len();
    let (start, end) = card_range(total, vp.window);
    let cards: Vec<Value> =
        order.get(start..end).unwrap_or_default().iter().filter_map(|(sid, aid)| m.agent(sid, aid)).map(|a| card(&m, &names, &labels, a)).collect();
    let mut cards_obj = Map::new();
    cards_obj.insert("total".into(), json!(total));
    cards_obj.insert("start".into(), json!(start));
    cards_obj.insert("virtualised".into(), json!(total > VIRTUALISE_ABOVE));
    cards_obj.insert("window".into(), Value::Array(cards));
    let mut view = json!({
        "revision": s.revision(),
        "daemonChips": daemon_chips(s, &m, &vp.daemons),
        "statusChips": status_chips(&m, &daemon_filter, &vp.statuses),
        "totals": {
            "tokens": totals.tokens,
            "cost": totals.cost,
            "covered": totals.covered,
            "of": totals.of,
            "label": format!("{} of {} agents", totals.covered, totals.of),
        },
        "cards": Value::Object(cards_obj),
        "drill": m.drilldown.as_ref().map(|d| drill(&m, &names, &labels, d)),
        "railCollapsed": vp.rail_collapsed,
        "groupBy": vp.group_by.name(),
    });
    if let (Some(groups), Some(obj)) = (&groups, view.as_object_mut()) {
        obj.insert("groups".into(), groups_block(groups, &names, start, end, total > VIRTUALISE_ABOVE));
    }
    Ok(view)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use pistudiodashboard_model::{AgentEntry, DaemonSlice, MonitorState};

    use super::*;

    fn seeded(n: usize) -> Session {
        let mut m = MonitorState::default();
        let mut d = DaemonSlice::new("e1", "box");
        d.server_id = Some("srv".into());
        m.daemons.push(d);
        for i in 0..n {
            m.upsert_agent(AgentEntry {
                server_id: "srv".into(),
                agent_id: format!("a{i:04}"),
                status: if i % 2 == 0 { AgentStatus::Running } else { AgentStatus::Idle },
                ..AgentEntry::default()
            });
        }
        Session::with_monitor(m)
    }

    #[test]
    fn empty_session_has_empty_lists_and_a_zero_label() {
        let v = Session::new().clone_view();
        assert_eq!(v["totals"]["label"], "0 of 0 agents");
        assert_eq!(v["cards"]["total"], 0);
        assert_eq!(v["daemonChips"], json!([]));
        assert_eq!(v["statusChips"], json!([]));
        assert_eq!(v["drill"], Value::Null);
    }

    fn window_of(s: &mut Session, start: u64, count: u64) -> (usize, u64, Value) {
        let v = s.execute("document.inspect", json!({"gridWindow": {"start": start, "count": count}})).unwrap();
        let n = v["cards"]["window"].as_array().unwrap().len();
        (n, v["cards"]["start"].as_u64().unwrap(), v)
    }

    #[test]
    fn up_to_100_cards_all_are_carried_and_above_that_only_the_reported_window_with_overscan() {
        for (total, virtualised) in [(99, false), (100, false), (101, true)] {
            let mut s = seeded(total);
            let (n, start, v) = window_of(&mut s, 0, 10);
            assert_eq!(v["cards"]["virtualised"], virtualised, "{total}");
            assert_eq!(v["cards"]["total"], total);
            assert_eq!(start, 0);
            assert_eq!(n, if virtualised { 30 } else { total }, "{total}: 10 reported + 20 overscan below, whole list otherwise");
        }
        let mut s = seeded(500);
        assert_eq!(window_of(&mut s, 200, 30).0, 70, "20 overscan on both sides");
        assert_eq!(window_of(&mut s, 200, 30).1, 180);
        assert_eq!(window_of(&mut s, 490, 30).0, 30, "clamped at the end: 470..500");
        assert_eq!(window_of(&mut s, 490, 30).1, 470);
        let (n, start, _) = window_of(&mut s, 0, 1000);
        assert_eq!((n, start), (200, 0), "never more than 200 cards per view");
        let (n, _, v) = window_of(&mut s, 9999, 5);
        assert_eq!(n, 20, "a window past the end still ends at the last card: 480..500");
        assert_eq!(v["cards"]["start"], 480);
        let none = s.execute("document.inspect", json!({})).unwrap();
        assert_eq!(none["cards"]["window"].as_array().unwrap().len(), 120, "no window reported yet: the first screenful 0..100 plus overscan");
        assert_eq!(none["totals"]["label"], "0 of 500 agents");
    }

    #[test]
    fn scrolling_reaches_every_card_of_500() {
        let mut s = seeded(500);
        let mut seen = std::collections::BTreeSet::new();
        let mut at = 0;
        while at < 500 {
            let v = s.execute("document.inspect", json!({"gridWindow": {"start": at, "count": 40}})).unwrap();
            for c in v["cards"]["window"].as_array().unwrap() {
                seen.insert(c["agentId"].as_str().unwrap().to_owned());
            }
            at += 40;
        }
        assert_eq!(seen.len(), 500);
    }

    #[test]
    fn cards_are_ordered_attention_running_error_idle_closed_and_closed_is_dimmed() {
        let mut m = MonitorState::default();
        let mut d = DaemonSlice::new("e1", "box");
        d.server_id = Some("srv".into());
        m.daemons.push(d);
        let mut add = |id: &str, status: AgentStatus, dialogs: u32| {
            let mut a = AgentEntry { server_id: "srv".into(), agent_id: id.into(), status, ..AgentEntry::default() };
            a.attention.pending_dialogs = dialogs;
            m.upsert_agent(a);
        };
        add("closed", AgentStatus::Closed, 0);
        add("idle", AgentStatus::Idle, 0);
        add("error", AgentStatus::Error, 0);
        add("running", AgentStatus::Running, 0);
        add("asking", AgentStatus::Idle, 1);
        let mut s = Session::with_monitor(m);
        let v = s.clone_view();
        let cards = v["cards"]["window"].as_array().unwrap();
        let ids: Vec<&str> = cards.iter().map(|c| c["agentId"].as_str().unwrap()).collect();
        assert_eq!(ids, ["asking", "running", "error", "idle", "closed"]);
        assert_eq!(cards[4]["dimmed"], true);
        assert_eq!(cards[3]["dimmed"], false);
        assert_eq!(cards[0]["attention"]["pendingDialogs"], 1);
        assert_eq!(cards[0]["statusLabel"], "idle");
        assert!(cards[0].get("dot").is_none(), "the frontend derives the dot from status + attention");
    }

    #[test]
    fn a_stale_daemon_dims_its_cards_with_the_time_it_went_stale() {
        let mut m = MonitorState::default();
        let mut d = DaemonSlice::new("e1", "box");
        d.server_id = Some("srv".into());
        d.stale_since = Some(1_700_000_000_000);
        m.daemons.push(d);
        m.upsert_agent(AgentEntry { server_id: "srv".into(), agent_id: "a".into(), status: AgentStatus::Running, stale: true, ..AgentEntry::default() });
        let v = Session::with_monitor(m).clone_view();
        let card = &v["cards"]["window"][0];
        assert_eq!((card["dimmed"].clone(), card["stale"].clone(), card["staleSince"].clone()), (json!(true), json!(true), json!(1_700_000_000_000_i64)));
    }

    #[test]
    fn an_unknown_status_counts_and_filters_with_idle() {
        let mut m = MonitorState::default();
        let mut d = DaemonSlice::new("e1", "box");
        d.server_id = Some("srv".into());
        m.daemons.push(d);
        for (id, status) in [("a", AgentStatus::Idle), ("b", AgentStatus::Unknown), ("c", AgentStatus::Running)] {
            m.upsert_agent(AgentEntry { server_id: "srv".into(), agent_id: id.into(), status, ..AgentEntry::default() });
        }
        let mut s = Session::with_monitor(m);
        let v = s.execute("document.inspect", json!({"filterStatuses": ["idle"]})).unwrap();
        assert_eq!(v["cards"]["total"], 2);
        assert_eq!(v["statusChips"], json!([{"bucket": "running", "count": 1, "selected": false}, {"bucket": "idle", "count": 2, "selected": true}]));
    }

    #[test]
    fn totals_and_counts_follow_the_filters_and_say_when_usage_is_partial() {
        use pistudiodashboard_model::{TokenUsage, UsageSnapshot};
        let mut m = MonitorState::default();
        for (entry, sid) in [("e1", "s1"), ("e2", "s2")] {
            let mut d = DaemonSlice::new(entry, entry);
            d.server_id = Some(sid.into());
            m.daemons.push(d);
        }
        for (sid, id, status, tokens) in
            [("s1", "a", AgentStatus::Running, Some(1200_u64)), ("s1", "b", AgentStatus::Idle, None), ("s2", "c", AgentStatus::Running, Some(300))]
        {
            let usage = tokens.map_or(UsageSnapshot::None, |total| UsageSnapshot::Some {
                tokens: Some(TokenUsage { total: Some(total), ..TokenUsage::default() }),
                cost: Some(0.5),
                context_usage: None,
                taken_at: 7,
            });
            m.upsert_agent(AgentEntry { server_id: sid.into(), agent_id: id.into(), status, usage, ..AgentEntry::default() });
        }
        let mut s = Session::with_monitor(m);
        let all = s.clone_view();
        assert_eq!((all["totals"]["label"].clone(), all["totals"]["tokens"].clone()), (json!("2 of 3 agents"), json!(1500)));
        let one = s.execute("document.inspect", json!({"filterDaemons": ["s1"], "filterStatuses": ["running"]})).unwrap();
        assert_eq!(
            (one["cards"]["total"].clone(), one["totals"]["label"].clone(), one["totals"]["tokens"].clone()),
            (json!(1), json!("1 of 1 agents"), json!(1200))
        );
        assert_eq!(
            one["statusChips"],
            json!([{"bucket": "running", "count": 1, "selected": true}, {"bucket": "idle", "count": 1, "selected": false}]),
            "counts respect the daemon filter, not the status filter"
        );
        let card_b = all["cards"]["window"].as_array().unwrap().iter().find(|c| c["agentId"] == "b").unwrap();
        assert_eq!(card_b["usage"], "none");
    }

    #[test]
    fn the_drill_carries_its_header_card_items_and_flags() {
        use pistudiodashboard_model::{DrillItem, DrillTarget, TurnKind};
        let mut s = seeded(2);
        let mut buffer = DrillBuffer {
            target: DrillTarget { server_id: "srv".into(), agent_id: "a0000".into() },
            state: DrillState::Refetching,
            stale: true,
            truncated_at_start: true,
            ..DrillBuffer::default()
        };
        buffer.items.push(DrillItem::Turn { kind: TurnKind::Started });
        buffer.items.push(DrillItem::Tool {
            call_id: "c".into(),
            kind: "shell".into(),
            label: "ls".into(),
            status: "running".into(),
            output_tail: Some(vec!["x".into()]),
        });
        buffer.items.push(DrillItem::Assistant { text: "<b>hi</b>\nthere".into() });
        s.update(|m| {
            m.drilldown = Some(buffer);
            Ok(())
        })
        .unwrap();
        let v = s.clone_view();
        let drill = &v["drill"];
        assert_eq!(drill["key"], json!({"serverId": "srv", "agentId": "a0000"}));
        assert_eq!((drill["state"].clone(), drill["stale"].clone(), drill["truncatedAtStart"].clone()), (json!("refetching"), json!(true), json!(true)));
        assert_eq!(drill["header"]["agentId"], "a0000");
        assert_eq!(drill["header"]["selected"], true);
        assert_eq!(drill["items"][0], json!({"type": "turn", "kind": "started"}));
        assert_eq!(drill["items"][1]["outputTail"], json!(["x"]));
        assert_eq!(drill["items"][2], json!({"type": "assistant", "text": "<b>hi</b>\nthere"}));
        let missing = DrillBuffer { target: DrillTarget { server_id: "srv".into(), agent_id: "gone".into() }, ..DrillBuffer::default() };
        s.update(|m| {
            m.drilldown = Some(missing);
            Ok(())
        })
        .unwrap();
        assert_eq!(s.clone_view()["drill"]["header"], Value::Null);
    }

    #[test]
    fn filters_select_chips_and_ill_typed_params_are_refused() {
        let mut s = seeded(4);
        let v = s.execute("document.inspect", json!({"filterDaemons": ["srv"], "filterStatuses": ["idle"], "railCollapsed": true})).unwrap();
        assert_eq!(v["cards"]["total"], 2);
        assert_eq!(v["daemonChips"][0]["selected"], true);
        assert_eq!(v["daemonChips"][0]["agentCount"], 4);
        assert_eq!(v["statusChips"], json!([{"bucket": "running", "count": 2, "selected": false}, {"bucket": "idle", "count": 2, "selected": true}]));
        assert_eq!(v["railCollapsed"], true);
        let none = s.execute("document.inspect", json!({"filterDaemons": ["ghost"]})).unwrap();
        assert_eq!(none["cards"]["total"], 0);
        for bad in
            [json!({"filterStatuses": ["bogus"]}), json!({"filterDaemons": "x"}), json!({"gridWindow": {"start": -1, "count": 1}}), json!({"railCollapsed": 1})]
        {
            assert!(matches!(s.execute("document.inspect", bad), Err(EngineError::BadParams(_))));
        }
    }

    /// `(agent id, cwd, status)` rows on one daemon `srv`.
    fn with_cwds(rows: &[(&str, Option<&str>, AgentStatus)]) -> Session {
        let mut m = MonitorState::default();
        let mut d = DaemonSlice::new("e1", "box");
        d.server_id = Some("srv".into());
        m.daemons.push(d);
        for (id, cwd, status) in rows {
            m.upsert_agent(AgentEntry {
                server_id: "srv".into(),
                agent_id: (*id).into(),
                cwd: cwd.map(str::to_owned),
                status: *status,
                ..AgentEntry::default()
            });
        }
        Session::with_monitor(m)
    }

    fn group_labels(v: &Value) -> Vec<(String, u64, u64)> {
        v["groups"]["window"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| (g["label"].as_str().unwrap().to_owned(), g["count"].as_u64().unwrap(), g["start"].as_u64().unwrap()))
            .collect()
    }

    #[test]
    fn flat_mode_adds_only_the_workspace_fields_and_no_groups() {
        let mut s = with_cwds(&[("a", Some("/work/a/app/"), AgentStatus::Idle), ("b", None, AgentStatus::Idle)]);
        let v = s.execute("document.inspect", json!({})).unwrap();
        assert_eq!(v["groupBy"], "none");
        assert!(v.get("groups").is_none());
        let by_id = |id: &str| v["cards"]["window"].as_array().unwrap().iter().find(|c| c["agentId"] == id).unwrap().clone();
        assert_eq!((by_id("a")["cwd"].clone(), by_id("a")["workspaceLabel"].clone()), (json!("/work/a/app"), json!("app")));
        assert_eq!((by_id("b")["cwd"].clone(), by_id("b")["workspaceLabel"].clone()), (json!(null), json!("No workspace")));
    }

    #[test]
    fn workspace_mode_groups_cards_and_describes_the_groups() {
        let mut s = with_cwds(&[
            ("n", None, AgentStatus::Running),
            ("a1", Some("/work/a/app"), AgentStatus::Idle),
            ("b1", Some("/work/b/app"), AgentStatus::Running),
            ("a2", Some("/work/a/app/"), AgentStatus::Closed),
        ]);
        let v = s.execute("document.inspect", json!({"groupBy": "workspace"})).unwrap();
        assert_eq!(v["groupBy"], "workspace");
        assert_eq!(group_labels(&v), [("b/app".to_owned(), 1, 0), ("a/app".to_owned(), 2, 1), ("No workspace".to_owned(), 1, 3)]);
        let ids: Vec<&str> = v["cards"]["window"].as_array().unwrap().iter().map(|c| c["agentId"].as_str().unwrap()).collect();
        assert_eq!(ids, ["b1", "a1", "a2", "n"], "group by group, card_order inside");
        assert_eq!(v["groups"]["total"], 3);
        assert!(v["groups"].get("layout").is_none(), "not virtualised");
        assert_eq!(v["groups"]["window"][1]["path"], "/work/a/app");
        assert_eq!(v["groups"]["window"][2]["path"], Value::Null);
        assert_eq!(v["cards"]["window"][0]["workspaceLabel"], "b/app", "a card's label is its header's");
        let flat = s.execute("document.inspect", json!({})).unwrap();
        assert_eq!((&flat["totals"], &flat["statusChips"]), (&v["totals"], &v["statusChips"]), "grouping changes neither totals nor chips");
    }

    #[test]
    fn filters_drop_empty_groups_and_change_counts() {
        let mut s =
            with_cwds(&[("a", Some("/w/one"), AgentStatus::Idle), ("b", Some("/w/two"), AgentStatus::Running), ("c", Some("/w/two"), AgentStatus::Idle)]);
        let v = s.execute("document.inspect", json!({"groupBy": "workspace", "filterStatuses": ["running"]})).unwrap();
        assert_eq!(group_labels(&v), [("two".to_owned(), 1, 0)]);
        assert_eq!(v["totals"]["of"], 1);
    }

    #[test]
    fn a_virtualised_grouped_view_carries_every_group_size_and_the_touched_headers() {
        // 500 agents in 200 workspaces (2 or 3 each), all idle.
        let ids: Vec<String> = (0..500).map(|i| format!("a{i:03}")).collect();
        let paths: Vec<String> = (0..500).map(|i| format!("/w/p{:03}", i % 200)).collect();
        let rows: Vec<(&str, Option<&str>, AgentStatus)> = ids.iter().zip(&paths).map(|(i, p)| (i.as_str(), Some(p.as_str()), AgentStatus::Idle)).collect();
        let mut s = with_cwds(&rows);
        let v = s.execute("document.inspect", json!({"groupBy": "workspace", "gridWindow": {"start": 250, "count": 40}})).unwrap();
        let layout: Vec<u64> = v["groups"]["layout"].as_array().unwrap().iter().map(|n| n.as_u64().unwrap()).collect();
        assert_eq!((layout.len(), layout.iter().sum::<u64>()), (200, 500));
        assert_eq!(v["groups"]["total"], 200);
        let (from, n) = (v["cards"]["start"].as_u64().unwrap(), v["cards"]["window"].as_array().unwrap().len() as u64);
        assert!(n <= 200);
        let heads = v["groups"]["window"].as_array().unwrap();
        let expected: Vec<u64> = {
            let mut first = 0;
            let mut out = Vec::new();
            for (i, c) in layout.iter().enumerate() {
                if first < from + n && first + c > from {
                    out.push(i as u64);
                }
                first += c;
            }
            out
        };
        assert_eq!(heads.len(), expected.len());
        for (h, i) in heads.iter().zip(&expected) {
            assert_eq!(h["label"], format!("p{i:03}"));
            assert_eq!(h["count"], layout[*i as usize]);
        }
        assert!(heads.first().unwrap()["start"].as_u64().unwrap() <= from, "the first header may start before the slice");
    }

    #[test]
    fn bad_group_by_is_refused_naming_the_values() {
        let mut s = with_cwds(&[]);
        for bad in [json!({"groupBy": "banana"}), json!({"groupBy": 3})] {
            let EngineError::BadParams(m) = s.execute("document.inspect", bad).unwrap_err() else { panic!("not bad params") };
            assert!(m.contains("none, workspace"), "{m}");
        }
    }

    #[test]
    fn hostile_workspaces_are_plain_short_labels() {
        let long = format!("/{}", "x".repeat(500));
        let mut s = with_cwds(&[("a", Some("/<img src=x onerror=alert(1)>"), AgentStatus::Idle), ("b", Some(long.as_str()), AgentStatus::Idle)]);
        let v = s.execute("document.inspect", json!({"groupBy": "workspace"})).unwrap();
        for g in v["groups"]["window"].as_array().unwrap() {
            assert!(g["label"].as_str().unwrap().chars().count() <= 80);
        }
        assert!(group_labels(&v).iter().any(|(l, ..)| l == "<img src=x onerror=alert(1)>"));
    }
    impl Session {
        fn clone_view(&mut self) -> Value {
            self.execute("document.inspect", Value::Null).unwrap()
        }
    }
}
