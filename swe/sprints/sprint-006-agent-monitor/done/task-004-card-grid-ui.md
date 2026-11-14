# Task 004 — Card grid UI

- **Sprint:** sprints/sprint-006-agent-monitor
- **Status:** done
- **Type:** feature
- **Area:** crates/shell (L6) + apps/pistudiodashboard/src (frontend)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-003, sprint-003-design-system/task-002

## Goal
Build the live card grid: cards, daemon and status filter chips, dimming, virtualisation above ~100 cards and the totals bar.

## Context / why
The main window's value: one card per live agent across daemons, filterable, stable-ordered, and cheap to render at 5,000 agents (D28).

## Scope references
- swe/specs/features/agent-monitor.md § View shape, § Card fields, § Card grid, § Status dot mapping, § Filters, § Virtualisation, § Usage display
- swe/specs/architecture/live-state.md § Derivations
- swe/specs/architecture/app-structure.md § UiState (filters, gridWindow), § Command registry (view.*)
- swe/specs/architecture/design-system.md § Components to rebuild
- `crates/shell/src/{view.rs,state.rs,commands.rs}` (monitor view assembly, window slice)
- `apps/pistudiodashboard/src/{App.tsx,lib/types.ts,test-support.ts}`
- `apps/pistudiodashboard/src/components/` (new `CardGrid.tsx`, `AgentCard.tsx`, `DaemonFilterRow` (from task-005 chips), `StatusFilterChips.tsx`, `StatusBar.tsx` update, css + tests)

## What to build
**Shell view assembly** (`view.monitor`, per agent-monitor § View shape): `statusChips[{bucket,count,selected}]` (counts respect the daemon filter; buckets are the five `AgentStatus` values; unknown buckets with idle), `totals {tokens?, cost?, covered, of, label}` from `usage_totals` over the filtered cards, `cards {total, virtualised, window: [Card]}`. `Card = {serverId, agentId, daemonName, title, model?, status, statusLabel, dimmed, stale, staleSince?, attention{pendingDialogs, errored}, currentTool?{label,status}, lastOutputLine?, usage, selected}` — no `dot` field: the frontend's `StatusDot` (sprint-003/task-002) derives the colour from `status` + `attention` (initializing→queued/accent, idle→muted, running→accent with ring, error→danger, closed→muted, Unknown→idle 'unknown'; pending dialog → warning > errored → danger > base); plain text only, already length-capped by the model. Closed also dimmed; stale dims and shows 'stale since <local time>' (time formatting in the frontend/local clock). Cards: `!archived`, daemon not a duplicate, passing both filters (AND across chip rows, OR within); order = `card_order` (sprint-002/task-001, task-002). Totals and status counts respect filters; the attention rail does not.
- **Virtualisation**: `VIRTUALISE_ABOVE = 100`: when `total > 100` the view carries only the window the frontend reported via `view.setGridWindow {start,count}` (`UiState.gridWindow`, clamped to the card count) with ±20 overscan, never serialising more than ~200 cards per emit; ≤ 100 cards → the whole list.
- **Commands**: `view.filterDaemons`, `view.filterStatuses` (from sprint-002) used by chips; 'clear filters' = both with empty lists; empty states: no daemons → 'Add daemon'; no agents; 'filters hide everything' with a clear-filters action.
- **Frontend**: `AgentCard` draws status dot (StatusDot primitive; colour never the only signal — status word shown), title, daemon name, model, current tool label + status, last output line, tokens/cost ('—' when `usage` none; compact numbers like 12.3k; cost bare number as supplied; context % when present; muted 'as of <time>' for old `takenAt`), small pending-request-count badge and 'stale' label; truncation with ellipsis (CSS), plain text only — never `dangerouslySetInnerHTML`/markdown. Card click → `agent.select` (drill-down is task-005/006; command may be disabled until then — render click as a registry call anyway). A windowed list (CSS grid with spacer + visible slice) reports the visible index range through `view.setGridWindow` on scroll (throttled). Status bar shows the totals ('N of M agents', tokens, cost). Animations respect reduced motion (running ring).

## Out of scope
- Drill-down panel (task-006), attention rail and card attention actions (sprint-007), interrupt/open-in-web-UI buttons (sprint-007), notifications (sprint-008), focus highlight (sprint-008/task-004).
- Design primitives (sprint-003/task-002).

## Acceptance criteria
- [x] Given agents in states attention/running/error/idle/closed, then cards render in exactly that order and the closed card is dimmed; given an `agent_archived`, then its card disappears. (D15)
- [x] Given streaming deltas on a running agent, then card positions do not change while only text fields update. (D15, D28)
- [x] Given two chips selected (a daemon and a status), then only matching cards remain and the totals and counts update; clearing restores all; empty filtered result shows the 'filters hide everything' state with a clear action. (D11, D15)
- [x] Given statuses `initializing/idle/running/error/closed`, then dots render accent/muted/accent-with-ring/danger/muted (closed never green, design-system.md) and a pending dialog overrides with the warning colour. (D6, D15, D20)
- [x] Given an agent with no live session, then usage reads '—' and the totals bar reads 'N of M agents' with N < M. (D16)
- [x] Given 500 agents on one daemon, then the view carries ≤ ~200 cards per emit and scrolling (reporting windows via `view.setGridWindow`) reaches all 500. (D28)
- [x] Given hostile strings (HTML, 1 MB title) in daemon data, then cards render truncated plain text. (D28)
- [x] Given a disconnected daemon, then its cards are dimmed with 'stale since <time>' and after the next Snapshot they refresh with no user action. (D25)
- [x] Given a pending `confirm` dialog on an agent, then its card shows the attention flag + warning dot and sorts first, clearing when `agent_ui_resolved` arrives (dialog reducers land in sprint-007; the view branch is test-covered with a hand-built state here). (D15, D36)
- [ ] Given a changed card for one agent, then a card status/tool/last-line change is visible in the emitted view within the 500 ms budget (D33) as measured in the sprint-009 load pass.

## Test / verification plan
- `cargo test -p pistudiodashboard-shell` (view assembly: order, filters, window slice at 99/100/101/500 cards, totals) and frontend component tests (card rendering, truncation, chips, windowed list reporting).
- Update `sampleView()` with cards in each state and a 150-card sample for the virtualised path.
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test` pass.
- `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` in BOTH themes (dark and light); open and look at each image (golden rule 5) and record what was checked in the task summary.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (agent-monitor): `needs_attention` is a sort rank/flag, not a filter chip.
- Filter state is not persisted in v1 [INFERENCE from brief].
- Card-height assumption for the window computation: use a fixed card height token from the design system; record it.
