# Task 003 — Monitor view: workspace on cards, `groupBy`, `groups` — Summary

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`document.inspect` (crates/engine/src/view.rs): optional `groupBy` param (`none`|`workspace`, else `BadParams` listing both); every card carries `cwd` (normalised or null) and `workspaceLabel` (labels computed once over the filtered scope; the drill-down header uses the same function, falling back to the base name for an agent a filter hides); the view always carries `groupBy`. In `workspace` mode cards are the concatenation of `grouped_order`, the existing `card_range` slices it, and `groups: {total, window: [{key, serverId, daemonName, label, path, count, start, needsAttention}], layout?}` is added (`window` = groups intersecting the sent slice; `layout` = every group's size, only when virtualised). Totals and status chips come from the same filtered scope, so they equal the flat values. `label_of` in ingest now returns `String` and is total. `docs/control-protocol.md` updated.

## Files created / changed
| File | Change |
|------|--------|
| `crates/engine/src/view.rs` | params, card fields, groups block, 6 tests |
| `crates/ingest/src/workspace.rs` | `label_of` total |
| `docs/control-protocol.md` | `groupBy`, card fields, `groups` |

## Build & test results
```
$ cargo test -p pistudiodashboard-engine -p pistudiodashboard-shell -> engine 50 passed, shell 20 passed
$ cargo clippy -p pistudiodashboard-engine -p pistudiodashboard-ingest --all-targets -- -D warnings -> clean
$ cargo xtask ci -> all 8 steps passed
```

## Smoke against pi-studio
Dev daemon `127.0.0.1:6790` (already running); agents made with `$PS run "hello" --cwd /tmp/gb-smoke/{a/app,b/app,solo}` and `#ui confirm` in `solo`. App: `target/debug/pistudiodashboard --headless --control 7878` with a throwaway `PISTUDIODASHBOARD_CONFIG_DIR=/tmp/gb-cfg` (auto-detected the daemon). Through the control socket, `engine.execute document.inspect {"groupBy":"workspace"}` returned 20 cards in 4 groups: `solo` (2, attention) · `tmp` (16, attention) · `a/app` (1) · `b/app` (1), with `start` 0/2/18/19; card labels matched their headers; `groupBy:"banana"` → `bad params: `groupBy` is one of: none, workspace; got `banana``. Observed: ordering by best rank then label among the two attention groups (`solo` < `tmp`).

## Acceptance criteria
- [x] Absent `groupBy` → flat view plus `groupBy:"none"` and card fields; existing view tests pass (`flat_mode_adds_only_the_workspace_fields_and_no_groups`)
- [x] `"banana"` refused naming `none, workspace`
- [x] `a/app`, `b/app`, `No workspace` last with correct count/start (`workspace_mode_groups_cards_and_describes_the_groups`)
- [x] 500 agents / 200 workspaces: window ≤ 200, layout 200 entries summing to 500, headers exactly those intersecting the slice
- [x] ≤ 100 cards: all headers, no `layout` (same test)
- [x] Filters drop empty groups; totals/status chips equal the flat values
- [x] Hostile cwd → plain labels ≤ 80 chars

## Follow-ups / TODO(verify)
- None new.
