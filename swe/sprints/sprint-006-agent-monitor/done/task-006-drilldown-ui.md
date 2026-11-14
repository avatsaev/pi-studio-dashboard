# Task 006 — Drill-down UI

- **Sprint:** sprints/sprint-006-agent-monitor
- **Status:** done
- **Type:** feature
- **Area:** apps/pistudiodashboard/src (frontend)
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-005, task-004

## Goal
Render the drill-down right panel (replacing the attention rail while open) with assistant text, reasoning, tool calls, turn markers and stale/loading states.

## Context / why
Lets the engineer watch one agent's current turn without becoming a second web UI (D6): plain-text rows, newest at the bottom, auto-follow unless scrolled up.

## Scope references
- swe/specs/features/agent-monitor.md § Drill-down panel (content, header, closing rules), § Card fields
- swe/specs/architecture/design-system.md § Components to rebuild (drill-down rows)
- swe/specs/architecture/live-state.md § `DrillItem`
- `apps/pistudiodashboard/src/{App.tsx,lib/types.ts,test-support.ts}`
- `apps/pistudiodashboard/src/components/DrillDownPanel.tsx` (+ `DrillRows`, css, test)
- `crates/shell/src/view.rs` (`view.monitor.drill` assembly)

## What to build
- **View** `view.monitor.drill? = {key, header: Card, items: [DrillItem], truncatedAtStart, state: loading|live|refetching, stale}` assembled in the shell from `MonitorState.drilldown` (items already capped at 2,000; the view may further send only the tail window if needed — keep ≤ the buffer).
- **Panel**: replaces the attention rail area while open (`railCollapsed` unaffected); header repeats the card (dot, title, daemon, model, usage) and has a close button (→ `agent.deselect`) and reserved slots for the Interrupt / Open-in-web-UI buttons and the agent's dialogs (filled by sprint-007; leave the slots empty — no dead buttons). Body, newest at the bottom, auto-follow unless the user scrolled up (a 'jump to latest' affordance when scrolled): assistant text (plain text, newlines preserved), reasoning (collapsed by default, labelled 'thinking'), tool calls (kind icon, label, status started|running|completed|error|canceled; running → live output tail of the last 12 lines; completed → one-line result summary), turn markers (started/completed/failed/canceled) and errors as rows, user messages and compaction as compact system rows. 'earlier output not shown' row when `truncatedAtStart`. `loading` → skeleton/spinner; `refetching` → subtle 'reloading' indicator; `stale` → banner 'daemon disconnected — showing last known output', no live updates. Esc closes (keyboard shortcut mapped to `agent.deselect` via the existing shortcuts table, Esc only when the drill is open). All daemon text is plain text, never HTML/markdown rendered; long lines wrap/truncate safely.
- `sampleView()` gets a drill sample (all row kinds, stale and loading variants, 2,000-item truncated variant).

## Out of scope
- Interrupt, dialog and open-in-web-UI controls (sprint-007/task-003, -005), and the attention rail (sprint-007/task-005).
- History browsing or paging upward.
- Markdown rendering.

## Acceptance criteria
- [x] Given a selected agent, when the drill-down opens, then it shows the fetched timeline then live deltas for assistant text, reasoning and tool calls with status, and replaces the attention rail; deselect (close button or Esc) restores the rail. (D6, D15)
- [x] Given a drilled agent beyond 2,000 items, then the panel shows at most 2,000 rows and an 'earlier output not shown' row. (D17)
- [x] Given `state == refetching`, then a 'reloading' indicator shows; given `stale`, then the stale banner shows and no new rows arrive; given `loading`, then the loading state shows.
- [x] Given hostile strings (HTML, huge output) in daemon data, then rows render as plain text without markup execution and without layout break. (D28)
- [x] Given the user scrolled up, then new rows do not yank the scroll; given they are at the bottom, then the panel follows.
- [x] Given Esc while the drill is open, then `agent.deselect` runs; Esc with no drill open does nothing.

## Test / verification plan
- Component tests for the panel (each row kind, states, truncation row, Esc, auto-follow logic with a mocked scroll container).
- `cargo test -p pistudiodashboard-shell` for the drill view assembly.
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test` pass.
- `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` in BOTH themes (dark and light); open and look at each image (golden rule 5) and record what was checked in the task summary.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (agent-monitor): plain text vs minimal markdown (default plain).
- Keep interrupt/dialog slots visually empty until sprint-007 so no non-functional control is shipped.
