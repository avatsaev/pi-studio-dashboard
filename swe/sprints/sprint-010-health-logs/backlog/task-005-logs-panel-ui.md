# Task 005 — Logs panel UI

- **Sprint:** sprints/sprint-010-health-logs
- **Status:** backlog
- **Type:** feature
- **Area:** frontend
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-003, task-004

## Goal
Render the live log tail with level selector, dropped counter and effective-level notice.

## Context / why
A virtualised monospace tail that can take 200 lines/s without redrawing faster than the engine's 100 ms view coalescing, and that never interprets log text as HTML.

## Scope references
- `swe/specs/features/daemon-health-and-logs.md` § Logs panel (display), § Privacy and logging, § Performance
- `swe/specs/architecture/design-system.md` § Components (badges, banner), § Scale tokens (`--pi-font-mono`, `font-size-2xs`)
- `apps/pistudiodashboard/src/components/` (new `LogsPanel.tsx` + css + test; entry action on the daemon chip/diagnostics menu)
- `apps/pistudiodashboard/src/lib/types.ts`
- `apps/pistudiodashboard/src/test-support.ts`

## What to build
- Entry point: "Logs" action invoking `logs.open {serverId}`; close invokes `logs.close`. Level selector (trace/debug/info/warn/error/fatal) invokes `logs.setMinLevel`.
- Rows: local time `HH:MM:SS.mmm` from `time`, level badge (trace/debug muted, info info, warn warning, error/fatal danger), message, expandable `fields` as pretty JSON (≤ 8 KB, already truncated by the engine), "— reconnected —" divider rows. Virtualised list. Auto-scroll to bottom unless the user scrolled up, with a "Jump to latest" button.
- Header: "daemon logs at <effectiveLevel>" notice (with the no-logs statement for `silent`), "N lines dropped by daemon" counter when `droppedTotal > 0`, disconnected banner, error text, degraded message as in the health panel.
- All text rendered as text nodes; no HTML. No logging of line content to the console.

## Out of scope
- State/commands — sprint-010/task-003.
- Persisting the level choice (TODO(verify)).

## Acceptance criteria
- [ ] Given a `sampleView()` with lines at several levels, then rows show time, level badge and message in order and `fields` expand to pretty JSON on click.
- [ ] Given a message containing HTML, then it renders as literal text.
- [ ] Given the level selector changed to `warn`, then `logs.setMinLevel {minLevel:"warn"}` is invoked.
- [ ] Given the user scrolled up and new lines arrive, then the viewport does not jump and "Jump to latest" appears; clicking it scrolls to the bottom and resumes auto-scroll.
- [ ] Given `effectiveLevel` above `minLevel` (including `silent`), then the header notice shows; given `droppedTotal > 0`, then the counter shows.
- [ ] Given 5,000 rows, then only a window of rows is mounted (virtualisation assertion).
- [ ] Given the close button, then `logs.close` is invoked.

## Test / verification plan
- Tests: component tests for each state (fake timers/scroll mocks). `cd apps/pistudiodashboard && npm run check && npm run lint && npm test`.
- Frontend: `cd apps/pistudiodashboard && npm run check && npm run lint && npm test` pass.
- Snapshots: `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` in **both** dark and light themes; open and look at each image (golden rule 5); record what was checked in the task summary.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- Snapshot states: populated tail, effective-level notice, dropped counter, reconnected divider — both themes.
- After this task, update `docs/control-protocol.md` with `health.*`/`logs.*` (if not already done by task-002/003).
