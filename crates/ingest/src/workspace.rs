//! Workspace grouping: an agent's workspace is its `cwd`. Pure functions of `MonitorState`, like
//! `card_order`; never stored. Paths are split by hand on both separators so the result is the same
//! on every platform, wasm included (the dashboard has no file access and resolves nothing).

use std::collections::{BTreeMap, BTreeSet};

use pistudiodashboard_model::{AgentEntry, MonitorState};

use crate::derive::{Filters, rank, scope, sort_key};

/// What a card and a group header say for an agent without a usable `cwd`.
pub const NO_WORKSPACE: &str = "No workspace";

/// Longest label, in characters (hostile input; the protocol crate already caps `cwd` itself).
pub const MAX_LABEL_CHARS: usize = 80;

/// One group of cards: a workspace on one daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceGroup {
    pub server_id: String,
    /// Normalised `cwd`; `None` for agents without one.
    pub workspace: Option<String>,
    pub label: String,
    /// `(server_id, agent_id)` in the `card_order` sort.
    pub cards: Vec<(String, String)>,
    /// Some card in the group has a pending dialog or an error.
    pub needs_attention: bool,
}

/// The workspace of a `cwd`: trimmed, trailing separators dropped (`/a/b/` is `/a/b`; a lone `/`
/// stays), `None` when nothing is left. Case-sensitive; `..` and symlinks are not resolved.
#[must_use]
pub fn workspace_of(cwd: Option<&str>) -> Option<String> {
    let mut s = cwd?.trim();
    while s.len() > 1 && s.ends_with(['/', '\\']) {
        s = &s[..s.len() - 1];
    }
    (!s.is_empty()).then(|| s.to_owned())
}

fn segments(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|s| !s.is_empty()).collect()
}

fn capped(s: String) -> String {
    match s.char_indices().nth(MAX_LABEL_CHARS) {
        Some((end, _)) => s[..end].to_owned(),
        None => s,
    }
}

/// Labels for `paths`: the last segment, with parent segments prepended until paths sharing a last
/// segment differ (`/work/a/app`, `/work/b/app` give `a/app`, `b/app`). Equal paths on two daemons
/// are one path here, so they share a label.
fn labels_of(paths: &BTreeSet<&str>) -> BTreeMap<String, String> {
    let segs: Vec<(&str, Vec<&str>)> = paths.iter().map(|p| (*p, segments(p))).collect();
    let mut depth = vec![1usize; segs.len()];
    let label = |i: usize, d: usize| -> String {
        let s = &segs[i].1;
        if s.is_empty() { segs[i].0.to_owned() } else { s[s.len().saturating_sub(d)..].join("/") }
    };
    // Each pass deepens every path whose label is shared and that still has a parent to add, so it
    // ends after at most as many passes as the longest path has segments.
    loop {
        let mut by_label: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, d) in depth.iter().enumerate() {
            by_label.entry(label(i, *d)).or_default().push(i);
        }
        let mut changed = false;
        for members in by_label.values().filter(|m| m.len() > 1) {
            for &i in members {
                if depth[i] < segs[i].1.len() {
                    depth[i] += 1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    (0..segs.len()).map(|i| (segs[i].0.to_owned(), capped(label(i, depth[i])))).collect()
}

/// Labels of every workspace in the filtered scope, keyed by normalised path. Both the cards and
/// the group headers read them, so a card's label equals its header's.
#[must_use]
pub fn workspace_labels(state: &MonitorState, filters: &Filters) -> BTreeMap<String, String> {
    let workspaces: Vec<String> = scope(state, filters).into_iter().filter_map(|(_, a)| workspace_of(a.cwd.as_deref())).collect();
    labels_of(&workspaces.iter().map(String::as_str).collect())
}

/// The label of `workspace` in `labels` (`NO_WORKSPACE` for `None`). A path outside the labelled
/// scope (an agent a filter hides but the drill-down still shows) gets its base name.
#[must_use]
pub fn label_of(labels: &BTreeMap<String, String>, workspace: Option<&str>) -> String {
    match workspace {
        None => NO_WORKSPACE.to_owned(),
        Some(w) => labels.get(w).cloned().unwrap_or_else(|| labels_of(&BTreeSet::from([w])).remove(w).unwrap_or_default()),
    }
}

/// A group being built: the daemon's config position and its cards.
type Bucket<'a> = (usize, Vec<(usize, &'a AgentEntry)>);

/// The cards grouped by `(daemon, workspace)`. Same scope as `card_order`; inside a group the
/// `card_order` sort; groups: every "No workspace" group last, then best card rank (needs
/// attention, running, error, idle, closed), daemon config position, label, path. A total order,
/// independent of event delivery order. No empty groups.
#[must_use]
pub fn grouped_order(state: &MonitorState, filters: &Filters) -> Vec<WorkspaceGroup> {
    let labels = workspace_labels(state, filters);
    let mut buckets: BTreeMap<(String, Option<String>), Bucket<'_>> = BTreeMap::new();
    for (pos, a) in scope(state, filters) {
        buckets.entry((a.server_id.clone(), workspace_of(a.cwd.as_deref()))).or_insert_with(|| (pos, Vec::new())).1.push((pos, a));
    }
    let mut groups: Vec<(u8, usize, WorkspaceGroup)> = buckets
        .into_iter()
        .map(|((server_id, workspace), (pos, mut cards))| {
            cards.sort_by(|(pa, a), (pb, b)| sort_key(*pa, a).cmp(&sort_key(*pb, b)));
            let best = cards.iter().map(|(_, a)| rank(a)).min().unwrap_or(u8::MAX);
            let label = label_of(&labels, workspace.as_deref());
            let cards = cards.into_iter().map(|(_, a)| (a.server_id.clone(), a.agent_id.clone())).collect();
            (best, pos, WorkspaceGroup { server_id, workspace, label, cards, needs_attention: best == 0 })
        })
        .collect();
    groups.sort_by(|(ba, pa, a), (bb, pb, b)| {
        (a.workspace.is_none(), *ba, *pa, &a.label, &a.workspace).cmp(&(b.workspace.is_none(), *bb, *pb, &b.label, &b.workspace))
    });
    groups.into_iter().map(|(_, _, g)| g).collect()
}

#[cfg(test)]
mod tests {
    use pistudiodashboard_model::{AgentEntry, AgentStatus, DaemonSlice};

    use super::*;

    fn daemon(m: &mut MonitorState, entry: &str, sid: &str) {
        let mut d = DaemonSlice::new(entry, entry);
        d.server_id = Some(sid.into());
        m.daemons.push(d);
    }

    fn agent(m: &mut MonitorState, sid: &str, id: &str, cwd: Option<&str>, status: AgentStatus) {
        m.upsert_agent(AgentEntry { server_id: sid.into(), agent_id: id.into(), cwd: cwd.map(str::to_owned), status, ..AgentEntry::default() });
    }

    #[test]
    fn workspace_of_trims_and_drops_trailing_separators() {
        assert_eq!(workspace_of(None), None);
        assert_eq!(workspace_of(Some("")), None);
        assert_eq!(workspace_of(Some("   ")), None);
        assert_eq!(workspace_of(Some("/a/b/")).as_deref(), Some("/a/b"));
        assert_eq!(workspace_of(Some(" /a/b//")).as_deref(), Some("/a/b"));
        assert_eq!(workspace_of(Some("/")).as_deref(), Some("/"));
        assert_eq!(workspace_of(Some("C:\\w\\app\\")).as_deref(), Some("C:\\w\\app"));
    }

    fn labels(paths: &[&str]) -> Vec<String> {
        let set: BTreeSet<&str> = paths.iter().copied().collect();
        let map = labels_of(&set);
        paths.iter().map(|p| map[*p].clone()).collect()
    }

    #[test]
    fn labels_use_the_base_name_and_add_parents_only_to_tell_paths_apart() {
        assert_eq!(labels(&["/home/u/proj/app"]), ["app"]);
        assert_eq!(labels(&["/work/a/app", "/work/b/app"]), ["a/app", "b/app"]);
        assert_eq!(labels(&["/work/a/app", "/work/b/app", "/x/a/app"]), ["work/a/app", "b/app", "x/a/app"]);
        assert_eq!(labels(&["/", "/tmp"]), ["/", "tmp"]);
        assert_eq!(labels(&["C:\\w\\app", "D:\\w\\app"]), ["C:/w/app", "D:/w/app"]);
    }

    #[test]
    fn a_hostile_path_gets_a_short_plain_label() {
        let long = format!("/{}", "é".repeat(100_000));
        let html = "/<img src=x onerror=alert(1)>";
        let l = labels(&[long.as_str(), html]);
        assert_eq!(l[0].chars().count(), MAX_LABEL_CHARS);
        assert_eq!(l[1], "<img src=x onerror=alert(1)>");
        let deep = "/a".repeat(5_000);
        assert!(labels(&[deep.as_str(), "/b"]).iter().all(|s| s.chars().count() <= MAX_LABEL_CHARS));
    }

    #[test]
    fn groups_order_attention_running_idle_then_no_workspace_last() {
        let mut m = MonitorState::default();
        daemon(&mut m, "e1", "s1");
        agent(&mut m, "s1", "idle1", Some("/w/idle"), AgentStatus::Idle);
        agent(&mut m, "s1", "run1", Some("/w/run"), AgentStatus::Running);
        agent(&mut m, "s1", "err1", Some("/w/err"), AgentStatus::Error);
        agent(&mut m, "s1", "bare", None, AgentStatus::Running);
        agent(&mut m, "s1", "ask1", Some("/w/ask"), AgentStatus::Idle);
        m.agents.get_mut(&pistudiodashboard_model::agent_key("s1", "ask1")).unwrap().attention.pending_dialogs = 1;
        let g = grouped_order(&m, &Filters::default());
        let order: Vec<&str> = g.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(order, ["ask", "run", "err", "idle", NO_WORKSPACE]);
        assert!(g[0].needs_attention && !g[1].needs_attention);
    }

    #[test]
    fn inside_a_group_the_card_order_sort_is_unchanged() {
        let mut m = MonitorState::default();
        daemon(&mut m, "e1", "s1");
        for (id, st) in [("a", AgentStatus::Closed), ("b", AgentStatus::Idle), ("c", AgentStatus::Running), ("d", AgentStatus::Idle)] {
            agent(&mut m, "s1", id, Some("/w/x"), st);
        }
        let flat = crate::card_order(&m, &Filters::default());
        let g = grouped_order(&m, &Filters::default());
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].cards, flat);
    }

    #[test]
    fn the_same_path_on_two_daemons_is_two_groups_in_config_order() {
        let mut m = MonitorState::default();
        daemon(&mut m, "e1", "s1");
        daemon(&mut m, "e2", "s2");
        agent(&mut m, "s2", "x", Some("/w/app/"), AgentStatus::Idle);
        agent(&mut m, "s1", "y", Some("/w/app"), AgentStatus::Idle);
        let g = grouped_order(&m, &Filters::default());
        assert_eq!(g.iter().map(|g| (g.server_id.as_str(), g.label.as_str())).collect::<Vec<_>>(), [("s1", "app"), ("s2", "app")]);
    }

    #[test]
    fn filters_remove_agents_before_grouping_and_empty_groups_vanish() {
        let mut m = MonitorState::default();
        daemon(&mut m, "e1", "s1");
        agent(&mut m, "s1", "a", Some("/w/one"), AgentStatus::Idle);
        agent(&mut m, "s1", "b", Some("/w/two"), AgentStatus::Running);
        let f = Filters { daemons: Vec::new(), statuses: vec![AgentStatus::Running] };
        let g = grouped_order(&m, &f);
        assert_eq!(g.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(), ["two"]);
        assert_eq!(workspace_labels(&m, &f).len(), 1, "labels follow the filtered scope");
    }
}
