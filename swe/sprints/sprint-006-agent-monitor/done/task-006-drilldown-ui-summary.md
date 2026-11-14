# Task 006 — Drill-down UI — Summary

- **Sprint:** sprints/sprint-006-agent-monitor
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`DrillDownPanel` (+ `DrillRows`): the right-hand panel that replaces the attention rail while `view.monitor.drill` is set (the rail is back when it is `null`; `railCollapsed` is untouched). Header repeats the card (dot, title, status word, daemon · model, `tokens · cost · context`, "stale since"), a close button (→ `agent.deselect`) and two **empty** reserved slots (`data-slot="actions"`, `data-slot="dialogs"`, `display:none` while empty: no dead control). Body, newest at the bottom, follows the end unless the user scrolled up (a "Jump to latest" button then appears; scrolling back within 24 px of the end resumes following; another agent starts at the end again). Rows: assistant text (plain, `pre-wrap`, newlines kept), reasoning (`<details>`, collapsed, "thinking"), tool calls (kind icon for shell/read/edit-write/search/fetch/other, label, status word; running → last 12 output lines; finished → one-line summary = last non-empty output line; border tinted by state), turn markers (started/completed/failed/canceled), errors, user messages and compaction as compact system rows (3-line clamp). `truncatedAtStart` → "earlier output not shown" row; `loading` → skeleton + "Loading output…"; `refetching` → "Reloading…"; `stale` → warning banner "daemon disconnected — showing last known output". All text goes through React text nodes and is clipped (20,000 chars per row, 300 per line).
**Esc**: `agent.deselect` is registered in the `Agent` menu with shortcut `Escape` (engine registry, parsed by the shell's shortcut table) and is enabled only while a drill is open; the webview's shortcut lookup now ignores *disabled* items, so Esc with no drill does nothing; Esc while a dialog is open is left to the dialog.

## Files created / changed
| File | Change |
|------|--------|
| `apps/pistudiodashboard/src/components/{DrillDownPanel,DrillRows}.tsx` + `.module.css` | created |
| `…/components/DrillDownPanel.test.tsx`, `…/src/App.monitor.test.tsx` | tests |
| `…/components/icons.tsx` | `ToolKindIcon` |
| `…/src/lib/shortcuts.ts` (+test) | `findShortcut` skips disabled items |
| `…/src/App.tsx` | drill vs rail, close button, dialog guard for Esc |
| `…/src/lib/types.ts` | `DrillItem`, `DrillState`, `DrillView` |
| `…/src/test-support-monitor.ts` | `drillItems()`, `withDrill()` (all row kinds, stale/loading/truncated variants are built in tests) |
| `crates/engine/src/agent_cmds.rs` | `agent.deselect`: menu `Agent`, shortcut `Escape` |
| `crates/engine/src/view.rs` | drill view test (`the_drill_carries_its_header_card_items_and_flags`); the drill assembly itself existed from task 005 |
| `crates/shell/src/monitor_tests.rs` | Esc row enabled only with a drill; drill in the shell view |

## How it satisfies the scope
agent-monitor § Drill-down panel, design-system § Drill-down rows (surface1 body, hairline, border tinted by state: accent running / success completed / destructive error / border canceled). Deviations: the "completed → one-line result summary" is derived from `outputTail` (the model keeps no separate result field); the panel is `1.5 × --pi-rail-width` (480 px) wide because rows are text-heavy. The 2,000-row cap is the model's ring; the panel draws what it is given (tested with 2,000).

## Build & test results
```
$ cd apps/pistudiodashboard && npm run check && npm run lint && npm test
tsc clean; oxlint --deny-warnings clean; 23 files, 203 tests passed, 0 failed
$ cargo test -p pistudiodashboard-engine -p pistudiodashboard-shell    -> 43 passed; 20 passed
$ cargo clippy -p pistudiodashboard-engine -p pistudiodashboard-shell --all-targets -- -D warnings   -> clean
```

## Acceptance criteria
- [x] drill opens with the fetched timeline then live deltas, replaces the rail; close button / Esc restore it (`App.monitor.test`; live below)
- [x] 2,000 rows + "earlier output not shown" (`DrillDownPanel.test`: 2,000 `<p>` + the row)
- [x] `refetching` → "Reloading…", `stale` → banner, `loading` → skeleton (tests + screenshots `drillstale`, `drillloading`)
- [x] hostile strings: `<img onerror>`/`<script>` stay text, 5 MB strings clipped, no layout break (test asserts no element created and < 100 kB of text; screenshot `drilltools` shows `<b>not bold</b> <img …>` as text, long rows wrap)
- [x] scroll up: new rows leave `scrollTop` alone; at bottom: follows (mocked scroll box test, incl. "Jump to latest", resume at bottom, new agent starts at end)
- [x] Esc with a drill → `agent.deselect`; Esc without one → nothing (`App.monitor.test`, `shortcuts.test`, shell `esc_is_the_close_shortcut…`)

## Smoke against pi-studio
Same headless app and dev daemon as task 004 (`--control 7906`). `agent.select` on "Refactor the parser into two modules" → `state: live`, items `system "Refactor…"`, `turn started`, `assistant "echo: Refactor…"`, `turn completed`, then (after `agent send`) four more rows; `#stream reasoning` → `system, turn started, reasoning "mock reasoning", assistant "echo: #stream reasoning", turn completed`; `#stream fail` → `…, turn failed, error "mock turn failure"`. While drilled on a `#stream reasoning` agent, `pi-studio agent send … "#stream reasoning again"` grew the buffer 5 → 7 → 9 items live while the header status went `running` → `idle`. Selecting an agent and archiving it from the CLI closed the drill and showed the toast `archived` (card count 116 → 115). The mock produces no tool calls, so tool rows are seen on synthetic items only.

## Screenshots (`done/screenshots/`, both themes)
- `s006-t006-grid-drill-*`: live data, real timeline of the refactor agent: header (dot, title, status word, daemon · model, usage line, close ×), system rows, turn dividers, assistant text; the selected card highlighted in the grid; rail gone.
- `s006-t006-drilltools-*`: real header, synthetic items: collapsed "thinking", assistant text, tool cards (read/search completed with one-line result, edit error with red border, shell running with the last 12 of 30 output lines, fetch canceled muted), error row, failed turn divider, hostile text as plain text.
- `s006-t006-drillstale-*`: real header, `stale` + `refetching` + `truncatedAtStart`: amber banner, "RELOADING…", "EARLIER OUTPUT NOT SHOWN".
- `s006-t006-drillloading-*`: skeleton bars + "LOADING OUTPUT…".
Defect found while looking and fixed: rows overlapped and tool cards were cut off because a column flex body shrinks children before it scrolls; fixed with `.body > * { flex: none }` and re-shot. Checked after: rows keep their height, output tail is exactly 12 lines, icons legible in both themes, success/danger/accent tints keep contrast on `surfaceSidebar` in light.

## Follow-ups / TODO(verify)
- Esc inside an open menu bar also closes the drill (the menu's own Esc handler does not mark the event).
- Sprint-007 fills the two header slots (Interrupt, Open in web UI, the agent's dialogs).
- Row keys are indexes (rows are append-only; a dropped-oldest shift at the 2,000 cap can flip an open "thinking" block).
