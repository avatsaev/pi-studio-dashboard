# Task 006 — Virtualised grouped layout (UI)

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Status:** done
- **Type:** feature
- **Area:** apps/pistudiodashboard/src (frontend)
- **Priority:** P2
- **Estimated size:** M
- **Depends on:** task-005

## Goal
With more than 100 cards in grouped mode, the grid scrolls through every group and card with exact spacers, reporting the visible card range through `view.setGridWindow` as in flat mode (W9).

## Context / why
Grouping adds variable-height sections (header + whole rows per group), so the flat row maths in `CardGrid.tsx` (`metricsFor`, `visibleRange`) no longer map scroll position to card index. The shell sends every group's size (`groups.layout`) so the frontend can compute exact offsets without the shell knowing column counts.

## Scope references
- `swe/specs/features/agent-workspace-grouping.md` § View additions (`layout`), § Frontend (virtualised grouped layout), acceptance "500 agents across 200 workspaces"
- `swe/specs/features/agent-monitor.md` § Virtualisation
- `apps/pistudiodashboard/src/components/CardGrid.tsx` (`metricsFor`, `visibleRange`, spacer rendering, report throttle), `CardGrid.test.tsx`
- `apps/pistudiodashboard/src/tokens.css` (`--pi-group-header-height` from task-005)

## What to build
- Pure, exported helpers (unit-testable, no DOM):
  - `groupOffsets(layout: number[], cols, rowHeight, headerHeight) -> { top: number[]; height: number[]; total: number }` — prefix sums; a group's height = header + `ceil(count/cols)` rows.
  - `visibleGroupedRange(scrollTop, viewHeight, offsets, layout, cols, rowHeight, headerHeight) -> { start, count }` — first and last visible **card indices** across groups (whole rows, one extra row like `visibleRange`).
- `CardGrid` grouped + `virtualised`: spacer above the first drawn group = its `top`; draw the groups in `groups.window`, each clipped to the cards present in `cards.window` (a partially sent group draws only whole rows that are present, with an inner spacer for the rows above); spacer below = `total` − bottom of the last drawn group. Report `visibleGroupedRange` through the existing throttled `onWindow`.
- Mode switch (`groupBy` changes): scroll to top and re-measure.

## Out of scope
- Any shell/engine change (the view shape is fixed by task-003). Collapse (W8).

## Acceptance criteria
- [ ] `groupOffsets`/`visibleGroupedRange` unit tests: single group equals the flat maths; many 1-card groups; a group taller than the viewport; scroll at top, middle and bottom; cols 1, 3, 5.
- [ ] Given a 500-card / 200-group sample view, scrolling (simulated scrollTop steps in tests) reports windows that together cover every card index, and drawn content never overlaps or leaves a gap (spacer + drawn height equals the computed total at each step).
- [ ] Resize (cols change) recomputes offsets and the reported range.
- [ ] Flat virtualised mode behaves exactly as before (existing tests unchanged).
- [ ] Snapshot of a long grouped list scrolled to the middle, both themes, looked at (golden rule 5).

## Test / verification plan
- `CardGrid.test.tsx` (helpers + grouped virtualised branch with a generated sample view).
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test`
- `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` both themes.
- `cargo xtask ci` before done.

## Notes
- Card height and header height are fixed tokens; never measure individual cards (keeps the maths exact and cheap).
- If exact partial-group drawing proves fragile, the fallback allowed by the spec is to draw whole groups that intersect the slice — but only if every such group's cards are within `cards.window`; otherwise keep the inner spacer approach.
