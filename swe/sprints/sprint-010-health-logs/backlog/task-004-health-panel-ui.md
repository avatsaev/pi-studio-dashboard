# Task 004 — Health panel UI

- **Sprint:** sprints/sprint-010-health-logs
- **Status:** backlog
- **Type:** feature
- **Area:** frontend
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-002, sprint-006-agent-monitor/task-006

## Goal
Render the per-daemon health panel from the engine's derived health view.

## Context / why
Values are formatted by derivation in Rust (spec: never in the frontend); the frontend lays them out in the pi-studio design language, in the right panel area shared with the rail and drill-down.

## Scope references
- `swe/specs/features/daemon-health-and-logs.md` § Health panel (display), § Degraded mode
- `swe/specs/architecture/design-system.md` § Components (badges, banner, tables), § Colour tokens
- `swe/specs/architecture/app-structure.md` § Webview boundary
- `apps/pistudiodashboard/src/components/` (new `HealthPanel.tsx` + css + test; daemon chip/diagnostics menu entry)
- `apps/pistudiodashboard/src/lib/types.ts`
- `apps/pistudiodashboard/src/test-support.ts` (`sampleView()` health variants)

## What to build
- Entry point: "Health" action on the daemon chip/diagnostics menu invoking `health.open {serverId}` (disabled with the engine's reason — "daemon too old" shows the "no observability (daemon too old)" badge); close button/Esc invokes `health.close`.
- Panel: version badge, uptime (formatted), `startedAt` local time, CPU %, memory (rss, heap used/total), event-loop lag with engine-provided tint level (normal/warning/danger), pid and Node version meta, agents-by-status counts in canonical order (+ "unknown"), clients table (clientType, appVersion or "—", short clientId, connected since, remote address, transport badge, "this dashboard"/"pi-studio dashboard" marks, self first, "+N more"), "No client data" row when empty, stale banner "stale since <time>" and error text states, degraded message "This daemon is too old for health and logs — update pi-studio".
- Plain text only; no logic beyond layout. Both themes via tokens only (token-integrity test must stay green).

## Out of scope
- Logs panel UI — sprint-010/task-005.
- State/derivation logic — sprint-010/task-002.

## Acceptance criteria
- [ ] Given a `sampleView()` health state, then version, uptime, CPU, memory, lag, agents by status and clients render with the right values and the self row is marked "this dashboard" and listed first.
- [ ] Given lag levels warning and danger, then the tint changes accordingly (class assertion).
- [ ] Given stale health, then the stale banner shows; given an error state, then the error text shows; given unsupported, then the single degraded message shows and no panel data.
- [ ] Given a status with missing fields, unknown transport and no clients, then the panel renders without errors.
- [ ] Given the close button or Esc, then `health.close` is invoked.

## Test / verification plan
- Tests: component tests for each state above. `cd apps/pistudiodashboard && npm run check && npm run lint && npm test`.
- Frontend: `cd apps/pistudiodashboard && npm run check && npm run lint && npm test` pass.
- Snapshots: `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` in **both** dark and light themes; open and look at each image (golden rule 5); record what was checked in the task summary.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- Snapshot states: normal, warning lag, stale, degraded — both themes.
- TODO(verify): panel placement vs the rail (spec tentative: replaces the rail like the drill-down).
