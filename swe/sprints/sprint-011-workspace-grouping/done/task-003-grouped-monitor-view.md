# Task 003 — Monitor view: workspace on cards, `groupBy` param, `groups` block

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Status:** done
- **Type:** feature
- **Area:** crates/engine (L5)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-001, task-002

## Goal
`document.inspect` sends every card's workspace, accepts a `groupBy` param, and in `workspace` mode orders cards group by group and describes the groups, including when the list is virtualised.

## Context / why
The frontend only draws what it is sent (golden rule 1). This task produces the exact view shape the grouped grid needs, reusing the existing windowing so a 5,000-agent view stays bounded (D28, W9).

## Scope references
- `swe/specs/features/agent-workspace-grouping.md` § Card additions, § View additions, § View (engine), § Error handling
- `swe/specs/features/agent-monitor.md` § View shape, § Virtualisation (amended)
- `crates/engine/src/view.rs` (`read_params`, `card`, `card_range`, `monitor_view`, `VIRTUALISE_ABOVE`, `OVERSCAN`, `MAX_WINDOW`, and its tests)
- `crates/ingest/src/derive.rs` (task-001 functions)
- `docs/control-protocol.md` (`document.inspect` params line)

## What to build
- `read_params`: optional `groupBy` (`"none"` | `"workspace"`, absent/null = `none`); any other value → `BadParams` naming the allowed values.
- `card()`: add `cwd` (normalised workspace or null) and `workspaceLabel` (task-001 labels, computed once per view over the filtered scope; `"No workspace"` when none). Both modes.
- `monitor_view`:
  - add top-level `groupBy`;
  - `none`: unchanged ordering via `card_order`; no `groups` key;
  - `workspace`: order = concatenation of `grouped_order` cards; the existing `card_range` slices it; add `groups: { total, window: [GroupHeader], layout? }` where
    `GroupHeader = { key, serverId, daemonName, label, path?, count, start, needsAttention }`, `key` a stable string from serverId + workspace (e.g. `"<serverId>\u0000<path or empty>"` — any injective encoding), `start` = index of the group's first card in the full ordered list, `window` = every group whose `[start, start+count)` intersects the sent `[start, end)` slice, and `layout = [count per group]` present only when `cards.virtualised`.
- Totals and status-chip counts are unchanged by grouping (same filtered scope).
- Update the `document.inspect` line in `docs/control-protocol.md` (`groupBy?`).

## Out of scope
- `UiState.groupBy`, `view.setGroupBy`, passing the mode from the shell (task-004); frontend (tasks 005–006).

## Acceptance criteria
- [ ] With `groupBy` absent, the view equals today's except for the added `groupBy: "none"` and the two card fields (existing view tests still pass after adding those fields).
- [ ] `groupBy: "banana"` is refused with a message listing `none, workspace`.
- [ ] Given agents in `/work/a/app`, `/work/b/app` and with no cwd, `groupBy: "workspace"` yields groups `a/app`, `b/app`, `No workspace` (last) with correct `count`/`start`, and `cards.window` in that grouped order.
- [ ] Given 500 agents in 200 workspaces and a grid window in the middle, then `cards.window` ≤ `MAX_WINDOW`, `groups.layout` has 200 entries summing to `cards.total`, and `groups.window` holds exactly the groups intersecting the slice (including partially intersecting ones at both edges).
- [ ] Given ≤ 100 cards, then `groups.window` lists every group and `layout` is absent.
- [ ] Daemon and status filters drop empty groups and change header counts; totals/status chips equal the ungrouped values for the same filters.
- [ ] Hostile `cwd` (HTML, max-size) appears as plain JSON strings, label ≤ 80 chars.

## Test / verification plan
- Extend tests in `crates/engine/src/view.rs` (pattern: `seeded(n)` helper; add cwd variety).
- `cargo test -p pistudiodashboard-engine`; `cargo clippy -p pistudiodashboard-engine --all-targets -- -D warnings`
- Smoke against pi-studio (see `swe/notes/smoke-harness.md`): create agents with `--cwd /tmp`, `--cwd /tmp/ws-a`, `--cwd /tmp/ws-b` (mkdir first) on the dev daemon, run `document.inspect {"groupBy":"workspace"}` through `pistudiodashboard-cli` or the control protocol, record the groups observed in the task summary.
- `cargo xtask ci` before done.

## Notes
- Labels must be computed over the same filtered scope in both modes so a card's label equals its header's label.
- The selected-card flag and drill-down are unaffected by grouping.
