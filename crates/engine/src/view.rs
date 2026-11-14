//! The monitor view returned by `document.inspect`: built only from `MonitorState` and the
//! ingest derivations. Camel-case JSON; the frontend draws exactly this (agent-monitor § View
//! shape). The engine has no clock, so "taken ago" strings are the frontend's job
//! (`usage.takenAt` is epoch ms).

use std::collections::BTreeMap;

use pistudiodashboard_ingest::{Filters, card_order, daemon_status_counts, usage_totals};
use pistudiodashboard_model::{AgentEntry, AgentStatus, DrillBuffer, DrillState, MonitorState, UsageSnapshot};
use serde_json::{Map, Value, json};

use crate::{EngineError, Result, Session, params};

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

fn card(m: &MonitorState, names: &Names<'_>, a: &AgentEntry) -> Value {
    let selected = m.drilldown.as_ref().is_some_and(|d| d.target.server_id == a.server_id && d.target.agent_id == a.agent_id);
    json!({
        "serverId": a.server_id,
        "agentId": a.agent_id,
        "daemonName": names.get(&a.server_id),
        "title": a.title.as_deref().unwrap_or(&a.agent_id),
        "model": a.model,
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

fn drill(m: &MonitorState, names: &Names<'_>, d: &DrillBuffer) -> Value {
    let state = match d.state {
        DrillState::Loading => "loading",
        DrillState::Live => "live",
        DrillState::Refetching => "refetching",
    };
    json!({
        "key": {"serverId": d.target.server_id, "agentId": d.target.agent_id},
        "header": m.agent(&d.target.server_id, &d.target.agent_id).map(|a| card(m, names, a)),
        "items": d.items.iter().map(|i| serde_json::to_value(i).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        "truncatedAtStart": d.truncated_at_start,
        "state": state,
        "stale": d.stale,
    })
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
    let order = card_order(&m, &filters);
    let totals = usage_totals(&m, &filters);
    let names = Names::new(&m);
    let total = order.len();
    let (start, end) = card_range(total, vp.window);
    let cards: Vec<Value> = order.get(start..end).unwrap_or_default().iter().filter_map(|(sid, aid)| m.agent(sid, aid)).map(|a| card(&m, &names, a)).collect();
    let mut cards_obj = Map::new();
    cards_obj.insert("total".into(), json!(total));
    cards_obj.insert("start".into(), json!(start));
    cards_obj.insert("virtualised".into(), json!(total > VIRTUALISE_ABOVE));
    cards_obj.insert("window".into(), Value::Array(cards));
    Ok(json!({
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
        "drill": m.drilldown.as_ref().map(|d| drill(&m, &names, d)),
        "railCollapsed": vp.rail_collapsed,
    }))
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

    impl Session {
        fn clone_view(&mut self) -> Value {
            self.execute("document.inspect", Value::Null).unwrap()
        }
    }
}
