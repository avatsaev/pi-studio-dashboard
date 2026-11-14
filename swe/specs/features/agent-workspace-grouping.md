# Feature — Agent Workspace Grouping (group cards by working directory)

> Part of: [overview.md](../overview.md)
> Dependencies: [agent-monitor.md](agent-monitor.md), [live-state.md](../architecture/live-state.md), [app-structure.md](../architecture/app-structure.md), [design-system.md](../architecture/design-system.md), [config-and-secrets.md](../architecture/config-and-secrets.md)
> Source: user request (2026-11-14): "group the agents by workspace (cwd) … right now I see all the agents but not sure which one belongs to which workspace". Not from the grilled brief; decisions below are numbered `W1`–`W9` and are this spec's own.

## Purpose
The card grid shows every agent as one flat list. The only per-card location hint is the daemon name, and agent titles are often a uuid or a first prompt, so an engineer cannot tell which project an agent works in. The daemon already reports each agent's working directory (`cwd` in `list_agents_response`); the dashboard stores it (`AgentEntry.cwd`) but never shows it. This feature (a) shows the workspace on every card and (b) adds an opt-in **group by workspace** mode that lays the grid out as one titled section per workspace.

Still a monitor: grouping is a view of what the daemon sent. No workspace/project management, no git status, no worktrees (those stay out of scope, overview *Not in scope*).

## Decisions
| # | Decision |
|---|----------|
| W1 | A **workspace** is an agent's `cwd`. The group key is `(serverId, normalised cwd)`: the same path on two daemons is two machines' directories, so it is two groups. |
| W2 | Grouping is a **mode**, default `workspace` (decided by the user on 2026-11-14: the app starts grouped). `none` is the flat grid as it was before this feature. Modes: `none`, `workspace`. Modelled as an enum so other groupings can be added without changing the command. |
| W3 | The mode is a user preference, **persisted** like the theme (`settings.groupBy`, absent = `workspace`). Daemon/status filters stay non-persisted. |
| W4 | Agents without a usable `cwd` form one group, **"No workspace"**, per daemon, ordered last. |
| W5 | Group order: best card rank in the group (needs-attention → running → error → idle → closed, the `card_order` rank), then daemon config position, then label ascending, then full path ascending. Inside a group: the existing `card_order` sort. `lastActivity` does **not** order groups, so section headers do not jump around while agents stream. |
| W6 | The workspace is always visible on the card, grouped or not (flat mode shows it in the card meta line; grouped mode omits it there because the header says it). |
| W7 | Filters (daemon, status) apply first; empty groups are not shown; header counts are the filtered counts. The attention rail is unaffected (as for filters today). |
| W8 | **Collapsing** (added 2026-11-14, see *Collapsing groups*): each group header is a button that collapses or expands its group; View menu has Collapse All / Expand All. Collapse state is session state (`UiState.collapsedGroups`), not saved. |
| W9 | Virtualisation stays: above 100 cards the view is windowed, and group layout is carried so the frontend can place headers and windows exactly. |

## Public contract

### Commands
All are shell (UI-only) commands like `view.setTheme`; registry-style ids, camelCase (golden rule 3).

| Command id | Args | Effect | Enabled / reason |
|------------|------|--------|------------------|
| `view.setGroupBy` | `{ groupBy: "none" \| "workspace" }` | Sets `UiState.groupBy`; requests a config save of `settings.groupBy` | always; any other value is refused with the message `` `groupBy` is one of: none, workspace `` (shell commands return messages, not codes) |

Menu: **View** menu, param-bound entries `Group: None` / `Group: By Workspace` with a check on the active one (same pattern as `view.setTheme:dark`). No default shortcut [TODO(verify): a shortcut is optional; none proposed, to avoid colliding with the Cmd+Shift+A rail toggle].

The control protocol's `ui.set` accepts `groupBy` (validated by the same field validator as `theme`); `ui.inspect` returns it.

### State
```
UiState.groupBy: "none" | "workspace"          # serde camelCase, default "workspace"
Config.settings.groupBy: "none" | "workspace"  # optional; absent or unknown value -> "workspace" (config issue reported for unknown, entry not dropped)
```
`UiState` is the runtime source of truth; `settings.groupBy` mirrors it. Plan-time finding (2026-11-14): no `UiState` preference is persisted yet (`theme` and `attentionRailCollapsed` exist in the config file but nothing writes or seeds them), so this feature adds the path itself: the shell command updates `UiState` and asks the engine to store the value in its `Config` and save it through the existing `config_store` hook (`Session::persist`); at startup `UiState.groupBy` is seeded from the loaded config. The same path can later carry `theme` and the rail flag (not in this feature).

### Card additions (always sent)
```
Card += { cwd?: string,            # normalised, <= 512 bytes (protocol cap), plain text
          workspaceLabel: string } # what the card and header display; "No workspace" when cwd is absent
```
Both are derived in L2 (pure) and serialised by the engine view; the frontend does no path parsing (golden rule 1).

### View additions (`view.monitor`)
```
view.monitor += {
  groupBy: "none" | "workspace",
  groups?: {                        # present only when groupBy == "workspace"
    total: n,                       # number of groups after filtering
    window: [GroupHeader],          # headers of the groups intersecting the sent card slice
    layout?: number[]               # card count of EVERY group, in group order; present only when cards.virtualised
  }
}
GroupHeader = { key: string,        # opaque, stable view-local id (React key), derived from serverId + cwd
                serverId, daemonName,
                label: string,      # "workspace name", see Label rules
                path?: string,      # full normalised cwd, for the tooltip/secondary text
                count: n,           # cards in the group after filtering
                start: n,           # index of the group's first card in the full ordered card list
                needsAttention: bool }   # any card in the group has pendingDialogs > 0 or errored
```
`cards.window` keeps its meaning: a slice of the **full ordered card list**, where "ordered" now means "group by group in W5 order, `card_order` inside". The frontend recovers the group boundaries from `groups.window[].start/count`; for a non-virtualised list (≤ 100 cards) `window` is the whole list and `groups.window` is every group. `groups.layout` has one integer per group; worst case is bounded by the card bound (≈ 5,000 cards ⇒ ≤ 5,000 integers per emit, view coalescing unchanged, D28).

In `groupBy: "none"` the view is byte-identical to today's plus the two new card fields.

## Behavior & algorithms

### Normalising the workspace (L2, pure)
```
function workspace_of(cwd: Option<String>) -> Option<String>:
    s = trim(cwd)
    if s is empty: return None
    while len(s) > 1 and s ends with '/' or '\\': s = drop_last(s)     # "/a/b/" == "/a/b"; root "/" stays "/"
    return s                                    # case-sensitive, no symlink/`..` resolution (the dashboard has no file access)
```

### Label rules
```
function base(path): last non-empty segment of path split on '/' and '\\'     # "/home/u/proj/app" -> "app"; "/" -> "/"
labels are computed over the groups in the current (filtered) view:
    group.label = base(path)
    # Disambiguate equal basenames: prepend parent segments until unique among the groups with that basename
    #   "/work/a/app", "/work/b/app" -> "a/app", "b/app"
    # If two groups differ only by daemon (same path on two daemons) they get the same label; the header's
    #   daemon name (shown whenever > 1 daemon has cards in the view) tells them apart.
    group.label capped at 80 chars (hostile input), ellipsis added by the frontend as well
    No workspace -> label "No workspace", path None
```

### Grouping and ordering (L2, `derive.rs`, pure functions of `MonitorState`, never stored)
```
function grouped_order(state, filters) -> Vec<Group{ key, server_id, cwd: Option<String>, cards: Vec<(sid, aid)> }>:
    cards  = scope(state, filters)                          # same scope as card_order: not archived, daemon known and not duplicate, filters applied
    groups = bucket cards by (server_id, workspace_of(card.cwd))
    for g in groups:
        g.cards   = sort(g.cards, by sort_key)             # the existing card sort, unchanged
        g.best    = min(rank(card) for card in g.cards)
    sort groups by (g.cwd.is_none(), g.best, daemon_pos, label, cwd)          # real workspaces first (W4), then W5
    return groups

card_order(state, filters)  stays as is (flat, used when groupBy == none)
grouped card list == concat(g.cards for g in grouped_order(...))
```
W4 puts every "No workspace" group after all real workspaces, so `cwd.is_none()` is the leading sort key; W5's `(best rank, daemon position, label, path)` orders the rest. Attention inside a "No workspace" group is still surfaced by the rail and by the card's own warning dot [TODO(verify): confirm that a "No workspace" group holding a pending dialog should still sort last; alternative is to rank it like any other group].

Group order is a total order over distinct groups and does not depend on event delivery order or on streaming deltas (only status/attention changes rank).

### View (engine, `view.rs`)
```
monitor_view(session, params):
    filters = ...                                          # as today
    if groupBy == none: order = card_order(..); groups = absent
    else:
        gs    = grouped_order(..)
        order = flatten(gs)
        total = len(order); (start, end) = card_range(total, window)     # unchanged windowing, over the group-ordered list
        groups.window = [header(g) for g in gs if g.range intersects [start, end)]
        groups.layout = [len(g.cards) for g in gs] if total > VIRTUALISE_ABOVE
    cards.window = order[start..end] -> card(..)           # card() gains cwd + workspaceLabel
```
Grouped mode keeps `MAX_WINDOW` (≈200 cards per emit) and `OVERSCAN`; a header is sent for every group touched by the slice even if only partially inside it.

### Frontend (draws only)
- `groupBy == none`: `CardGrid` unchanged; each `AgentCard` meta line gains the workspace label (`title` attribute = full path) after the daemon name.
- `groupBy == workspace`: each group renders as `<section aria-label=label>` with a header row (workspace label, muted full path, daemon name when more than one daemon contributes cards, count badge, warning badge if `needsAttention`) followed by its own card grid using the same column metrics as today. A group always starts on a new row; the last row of a group may be partial. Cards omit the workspace line.
- Virtualised grouped layout: the frontend knows every group's count (`layout`), the column count and two fixed heights (card row = `--pi-card-height` + gap, header = new token `--pi-group-header-height`), so it computes each group's offset by prefix sum, draws spacers before/after the sent slice, and reports the visible **card index** range through `view.setGridWindow` as today (the shell never learns column counts).
- Switching mode resets scroll to the top and sends a fresh window.
- Header is a button (`aria-expanded`) that collapses or expands the group, W8.

## Data & persistence touchpoints
- Reads `AgentEntry.cwd` (set from `list_agents_response`, capped 512 bytes in the protocol crate, trimmed/normalised only at derivation, never mutated in the model). `agent_update` carries no `cwd` (shipped fixtures `agent_update*.json`: status, labels, title, model), so an agent's workspace is stable for its lifetime; a reconnect rebuild replaces it. [TODO(verify): confirm daemons never move an agent to another cwd.]
- Writes `UiState.groupBy` and `Config.settings.groupBy` (additive optional key; older config files load unchanged; a newer file read by an older build ignores the unknown key per the config spec's lenient rules).
- No new model fields, no new protocol requests, no new daemon capability, no pi-studio change.

## Error handling & edge cases
| Condition | Expected behavior |
|-----------|-------------------|
| Agent has no `cwd` / empty / whitespace | "No workspace" group; label "No workspace"; flat-mode card shows the same label |
| `cwd` with trailing separators, e.g. `/a/b/` and `/a/b` | One group (normalised) |
| `cwd` `/` | Label `/`; its own group |
| Same basename, different paths | Labels disambiguated by parent segments; full path in header/tooltip |
| Same path on two daemons | Two groups; header shows the daemon name; labels equal |
| 1 MB / HTML / control characters in `cwd` | Capped at 512 bytes by the protocol crate, label capped at 80, rendered as plain text, never as markup |
| Filter hides every agent of a workspace | Group disappears; no empty headers |
| All agents archived/closed in a workspace | Group follows the usual card rules (closed cards stay, dimmed; archived are absent, so an all-archived group vanishes) |
| Agent changes status (e.g. starts needing attention) | Its group may move up (W5); cards do not reorder on stream deltas |
| Selected agent's group disappears (filter) | Selection and drill-down unaffected (same as filters today) |
| Daemon disconnects | Its groups stay with stale cards dimmed, as in the flat grid |
| Daemon entry removed / becomes duplicate | Its groups vanish with its cards |
| `view.setGroupBy` with an unknown value | Refused with the reason; state unchanged |
| Config has `groupBy: "banana"` | Defaults to `workspace`, issue reported, rest of the file loads |
| 500 agents in 200 workspaces | Virtualised; `layout` has 200 integers; ≤ ~200 cards per emit; scrolling reaches every group |
| Single group only | Still shows its header (grouped mode is explicit; no auto-collapse to flat) |

## Dependencies on other specs
- [agent-monitor.md](agent-monitor.md) — `Card`, `card_order`, filters, virtualisation (`view.setGridWindow`), view shape; this spec amends the view shape and card fields.
- [live-state.md](../architecture/live-state.md) — `AgentEntry.cwd`, derivation purity; new derivation `grouped_order` lives beside `card_order`.
- [app-structure.md](../architecture/app-structure.md) — `UiState`, shell commands, generated menus, control protocol `ui.set`.
- [config-and-secrets.md](../architecture/config-and-secrets.md) — `settings` schema, lenient load, debounced save.
- [design-system.md](../architecture/design-system.md) — tokens (`--pi-group-header-height`), snapshot checks in both themes.

## Collapsing groups (W8)
| Item | Contract |
|---|---|
| State | `UiState.collapsedGroups: string[]` of group keys (`groups.window[].key`), at most 5,000, distinct, 1–1024 characters; `ui.set {collapsedGroups}` validates the same. Keys of groups that no longer exist are harmless; `Collapse All` replaces the set with the current groups' keys. |
| Commands | `view.toggleGroup {key}` (no menu), `view.collapseAllGroups`, `view.expandAllGroups` (View menu). All disabled with "the agents are not grouped by workspace" when `groupBy` is `none`; Expand All also with "no group is collapsed". |
| `document.inspect` | takes `collapsedGroups` (keys) and `includeGroupKeys` (adds `groups.keys`, what Collapse All reads). A collapsed group's cards are **not** in `cards.window` / `cards.total` (so virtualisation follows what is drawn); totals and status chips are unchanged. |
| Header | gains `index` (position among all groups), `shown` (cards in the list: `count`, or 0 when collapsed) and `collapsed`; `count` stays the real number of agents. `layout[i]` is `shown`. A group with `shown: 0` belongs to a card slice `[start, end]` when its `start` (the list index where it sits; a run of collapsed groups shares one) is inside it. |
| Frontend | a collapsed group is a title only (32 px, no card grid); its count and "needs attention" badge stay, so a collapsed group never hides an agent that needs the engineer. With every group collapsed the titles are shown, not the empty state. The scroll maths asks the shell for the card index where a screen of collapsed titles sits. |

## Acceptance criteria
- [ ] Given agents with different `cwd`s, when `groupBy` is `none`, then each card shows its workspace label (full path on hover) and the grid and order are exactly as before. (W2, W6)
- [ ] Given agents in `/work/a/app`, `/work/b/app` and no cwd, when `view.setGroupBy {groupBy:"workspace"}` runs, then the grid shows three sections labelled `a/app`, `b/app` and `No workspace` (last), each with its own count, and cards do not repeat the workspace line. (W1, W4, W6)
- [ ] Given `/a/b` and `/a/b/`, then they are one group. (normalisation)
- [ ] Given the same path on two daemons, then there are two groups and each header names its daemon. (W1)
- [ ] Given a group with a running agent and a group with a pending dialog, then the attention group comes first, and an idle-only group comes after both; within a group the card order equals today's `card_order` restricted to it. (W5)
- [ ] Given streaming deltas on a running agent (no status change), then neither groups nor cards move. (W5)
- [ ] Given daemon and status filters, then groups with no matching card disappear and header counts, totals and status-chip counts follow the filters; the attention rail still shows everything. (W7)
- [ ] Given 500 agents across 200 workspaces, then the view carries ≤ ~200 cards, `groups.layout` has one entry per group, and scrolling shows every group header and card without gaps or overlap at several window widths. (W9)
- [ ] Given hostile `cwd` strings (HTML, 1 MB), then labels are truncated plain text. (hostile input, golden rule 2)
- [ ] Given `view.setGroupBy` with `"banana"`, then it is refused with the reason and `UiState` is unchanged; `ui.set {groupBy:"workspace"}` and the View menu entry reach the same state.
- [ ] Given `groupBy: "none"` chosen, when the app restarts, then it starts flat; given a config file without the key (or a new install), then it starts grouped by workspace with no warning; given `groupBy:"banana"`, then it starts grouped and reports a config issue. (W2, W3)
- [ ] `npm run snapshot` of grouped mode (dark and light) is inspected: headers, counts, attention badge, partial last row, `No workspace` group. (golden rule 5)
- [ ] `cargo xtask ci` is green, including the wasm and layers steps (no new dependency, no filesystem access).

## Out of scope / deferred
- Remembering the collapsed groups across restarts (they are session state, like the filters).
- Grouping by something other than `cwd` (daemon, model, label); `groupBy` is an enum so it can grow.
- Resolving worktrees to their main repository, git branch/status per workspace, workspace-level actions (git/worktrees remain out of scope).
- Showing workspace in the notification text (notification content stays content-free, D9).

## TODO(verify)
- Shortcut for `view.setGroupBy` (none proposed).
- Whether a "No workspace" group with a pending dialog should still sort last (W4/W5 as written: last).
- Whether daemons ever change an agent's `cwd` after creation (no shipped message carries it).
- Persisting `groupBy` (W3) versus treating it like the filters (session-only): written as persisted because it is a layout preference like the theme; confirm.
- Windows-style `cwd` paths: only separator normalisation is specified (Windows is out of scope, D34); confirm no labels needed beyond splitting on `\\`.
