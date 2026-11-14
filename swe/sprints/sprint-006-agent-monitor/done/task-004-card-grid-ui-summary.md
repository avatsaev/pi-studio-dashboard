# Task 004 — Card grid UI — Summary

- **Sprint:** sprints/sprint-006-agent-monitor
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
The live card grid. **View assembly** (`crates/engine/src/view.rs`, the `document.inspect` result the shell serves as `ui.inspect.document`; there is no `crates/shell/src/view.rs`): cards no longer carry `dot` (the frontend's `StatusDot` derives it from `status` + `attention`) but `statusLabel` and `staleSince`; the card window is now `cards.start` + `window`: up to 100 cards the whole list, above that the reported `gridWindow` ±20 overscan, never more than 200 cards (`card_range`); an `unknown` status counts and filters with `idle`; status chips are the five known buckets. **Frontend**: `AgentCard` (dot, title, daemon, model, current tool + status, last output line, `12.3k tokens` / `cost 0.42` / `38% context`, `—` without usage, muted "as of <time>" for an old snapshot, `N pending` badge, "stale since <time>", edge bar by state, plain text clipped to 400 chars and ellipsised by CSS), `CardGrid` (fixed card height, whole-row window maths, spacers above/below the drawn slice, visible range reported through `view.setGridWindow` throttled to one call per 100 ms with the last position always sent, empty states: no daemons → "Add daemon", no agents, "No agents match the filters." → "Clear filters"), `StatusFilterChips` (toggle chips with counts, "Clear filters" while any filter is on), `StatusBar` shows `N of M agents · tokens · cost`. Card click → `agent.select` through `engine.execute`. `view.filterStatuses`/`view.filterDaemons`/`view.setGridWindow` existed (sprint 002); the daemon chip's filter toggle is ImplUiS005's `DaemonChips` (task 005).

## Files created / changed
| File | Change |
|------|--------|
| `crates/engine/src/view.rs` | card shape (`statusLabel`, `staleSince`, no `dot`), `card_range` window/overscan/cap, `cards.start`, idle+unknown folding, 7 new tests |
| `crates/shell/src/monitor_tests.rs` (+ `mod` in `lib.rs`) | created: window/filters/Esc through the shell |
| `apps/pistudiodashboard/src/components/{AgentCard,CardGrid,StatusFilterChips}.tsx` + `.module.css` | created / rewritten (skeleton replaced) |
| `…/components/{AgentCard,CardGrid,StatusFilterChips}.test.tsx` | tests |
| `…/components/StatusBar.tsx`, `StatusBar.module.css` | totals text; segment max-width 320 → 560 px |
| `…/src/lib/{format.ts,format.test.ts,useNow.ts}` | created (compact numbers, clock, clip; local clock as state) |
| `…/src/lib/types.ts` | `Card` (+`statusLabel`, `staleSince`, −`dot`), `CardWindow.start`, `DrillView` types, `DotKind` removed |
| `…/src/{App.tsx,test-support.ts,App.test.tsx}`, `test-support-monitor.ts` (new), `App.monitor.test.tsx` (new) | wiring (`run`, status chips, grid callbacks), samples (`richView`, `bigView`), tests |
| `…/src/tokens.css`, `tokens.pin.json` | `--pi-card-height: 128px` (dashboard-local) |
| `docs/control-protocol.md` | card window / card shape documented |

## How it satisfies the scope
- agent-monitor § View shape, Card fields, Card grid, Filters, Virtualisation, Usage display: as above. Order is `card_order` (sprint-002); the frontend never reorders.
- **Card height assumption:** one fixed `--pi-card-height` = 128 px (grid rows `grid-auto-rows`, read by `CardGrid` with a 128/280/12 fallback when no style sheet is loaded). Columns = `floor((width+12)/(280+12))`, row pitch 140 px.
- Deviations: (1) the spec's `statusChips` "unknown with idle" is implemented as folding in the engine view; (2) `StatusBar` text segment is wider than design-system's 320 px (it now carries daemons + totals + tokens + cost); (3) a `Clear filters` button also sits in the chip row (besides the empty state) while any filter is on.

## Build & test results
```
$ cd apps/pistudiodashboard && npm run check && npm run lint && npm test
tsc clean; oxlint --deny-warnings clean; 23 files, 203 tests passed, 0 failed
$ cargo test -p pistudiodashboard-engine -p pistudiodashboard-shell
engine 43 passed (10 in view); shell 20 passed (3 new in monitor_tests)
$ cargo test -p pistudiodashboard -p pistudiodashboard-testkit   -> 29 passed, 5 passed
$ cargo clippy -p pistudiodashboard-engine -p pistudiodashboard-shell --all-targets -- -D warnings   -> clean
$ cargo fmt --check -p pistudiodashboard-engine -p pistudiodashboard-shell -> clean
```
`cargo xtask ci` not run (parent runs the full gate).

## Acceptance criteria
- [x] attention/running/error/idle/closed order, closed dimmed, archived removed (engine `cards_are_ordered_…`; live: archiving an agent on the daemon removed its card, 116 → 115)
- [x] streaming deltas keep card positions (`CardGrid` test re-renders with changed text, same order; `card_order` ignores stream, sprint-002)
- [x] daemon + status chip → matching cards, totals and counts follow; clearing restores; empty result shows "No agents match the filters." with a clear action (engine `totals_and_counts_…`, shell `both_filter_rows_…`, `CardGrid`/`App.monitor` tests; live screenshots `filter`, `hidden`)
- [x] dots queued/idle/ring/danger/muted, pending dialog warning override (`AgentCard.test` over all statuses; `StatusDot` tests; live pending-dialog cards)
- [x] no live session → `—`; totals "N of M agents" with N < M (`AgentCard.test`, engine test "2 of 3 agents")
- [x] 500 agents → ≤ 200 cards per view, scrolling reaches all (engine `up_to_100_…`, `scrolling_reaches_every_card_of_500`; shell `the_reported_grid_window_…`; `CardGrid` closed-loop test scrolls a harness that slices like the backend and sees all 500)
- [x] hostile strings (HTML, 1 MB title) → clipped plain text (`AgentCard.test`; screenshot `rich`)
- [x] disconnected daemon → dimmed + "stale since <time>" (engine `a_stale_daemon_dims…`, `AgentCard.test`; screenshot `rich`). "Refresh after the next Snapshot with no user action" is ingest behaviour (sprints 002/004); not re-run live (would need killing the shared dev daemon) → TODO(verify)
- [x] pending confirm → attention flag + warning dot, sorts first (live: 3 real `#ui confirm/select` agents sorted first with `1 pending`). Clearing on `agent_ui_resolved` is sprint-007's reducer; the view branch is covered with hand-built state
- [ ] 500 ms card-change latency: measured in the sprint-009 load pass (not claimed here)

## Smoke against pi-studio
Dev daemon `127.0.0.1:6790` (mock provider), 16 agents made with the pi-studio CLI (`hello`, `#stream fail`, `#stream error`, `#stream reasoning`, `#ui confirm` ×2, `#ui select`, `#ui notify`, `#ui multi`, `#todo plan` ×2, plus three prose prompts), then 100 more (`load agent N`) for the virtualised path, all archived again afterwards (daemon back to 16).
```
./target/debug/pistudiodashboard --headless --control 7906 --control-token-file /tmp/s006/control.token --config-dir /tmp/s006/cfg
# auto-detected local daemon -> live; python JSON-lines client: ui.inspect / engine.execute
```
Observed: `16 of 16 agents`, cards ordered errored/pending (3 `waiting` cards, 2 `error`) → running → idle; `agent send` follow-ups appeared as `lastOutputLine` "echo: …" on the cards within the poll; 116 agents → `virtualised:true`, default window 0..100 gave 116 cards (0..120), `view.setGridWindow {60,18}` → `start 40`, 58 cards; filters `["running","error"]` + daemon → 5 cards "5 of 5 agents"; `["closed"]` → 0 cards, chip `closed 0` selected. `scrollHeight` 8132 px for 116 cards = 58 rows × 140 − 12 + paddings (exact).
Observed gap (not this task): the app never ticks stats (`grep feed.tick apps/…/src-tauri/src` finds nothing), so usage stays at its first snapshot and cards show "as of <time>"; mock tokens are 0 and have no cost anyway.

## Screenshots (`swe/sprints/sprint-006-agent-monitor/done/screenshots/`, both themes each)
Commands: `npm run build`, then `npm run snapshot -- --view <ui.inspect json from the live app> --out … --size 1180x720`.
- `s006-t004-grid-after-*`: live data, 16 agents: error/pending cards first with red/amber edge bars, then idle with `echo:` last lines; chip rows (`Local 16 Connected`, `running 3 · error 2 · idle 11`), status bar `16 of 16 agents · 0 tokens · cost 0`. Looked at: contrast of muted text on cards (readable in both themes), error tint, badge colour; no overflow.
- `s006-t004-grid-running-*`: live data mid-turn: running ring dots with accent edge.
- `s006-t004-grid-virtualised-*`: live, 116 agents, scrolled to row 30 of a 58-row list through the real window (spacers exact; cards `load agent …`).
- `s006-t004-rich-*`: real cards overlaid with states the mock cannot produce (synthetic fields): tool line, usage `12.3k tokens cost 0.42 38% context`, `as of`, initializing (accent dot), unknown (outlined dot), closed (dimmed, muted), stale (dimmed + amber "stale since"), hostile HTML title / 500-char tool label clipped to one line.
- `s006-t004-filter-*`, `s006-t004-hidden-*`: live; two selected chip rows with "Clear filters"; "No agents match the filters." with its button.
Defects found and fixed during review: none visual in the grid; a flex-shrink defect in the drill panel was found (task 006).

## Follow-ups / TODO(verify)
- Live stale→reconnect→Snapshot refresh not re-run (shared dev daemon).
- The `feed.tick` owner-loop wiring is missing in the app (see above): report to the owner of sprint-005/001 follow-ups.
- Title ellipsis is aggressive on 280 px cards that also carry the "N pending" badge and the status word.
