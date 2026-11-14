# Task 005 — Workspace on cards and grouped sections (UI, non-virtualised)

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Status:** done
- **Type:** feature
- **Area:** apps/pistudiodashboard/src (frontend)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-004

## Goal
Cards show their workspace; with `groupBy: "workspace"` and ≤ 100 cards the grid draws one titled section per group, in the order the shell sent.

## Context / why
This is the user-visible fix for "I can't tell which agent belongs to which workspace" (W6), plus the grouped layout for the common case (no virtualisation).

## Scope references
- `swe/specs/features/agent-workspace-grouping.md` § Frontend (draws only), § Error handling, § Acceptance criteria
- `swe/specs/architecture/design-system.md` (tokens, snapshot checks)
- `apps/pistudiodashboard/src/lib/types.ts` (`Card`, `MonitorView`, `UiState`)
- `apps/pistudiodashboard/src/components/AgentCard.tsx` (+ `.module.css`, tests), `CardGrid.tsx` (+ css, tests)
- new `apps/pistudiodashboard/src/components/WorkspaceHeader.tsx` (+ css, test)
- `apps/pistudiodashboard/src/tokens.css` (`--pi-group-header-height`)
- `apps/pistudiodashboard/src/test-support-monitor.ts` / `test-support.ts` (sample views)
- `apps/pistudiodashboard/src/App.tsx` (prop plumbing only)

## What to build
- Types: `Card.cwd: string | null`, `Card.workspaceLabel: string`; `MonitorView.groupBy: "none" | "workspace"`, `MonitorView.groups?: { total; window: GroupHeader[]; layout?: number[] }`; `UiState.groupBy`.
- `AgentCard`: in flat mode, the meta line shows the workspace label after the daemon name (muted, clipped, `title` = full path, plain text). A `showWorkspace` prop (false in grouped mode).
- `WorkspaceHeader`: label (clipped), muted full path, daemon name only when the view has cards from more than one daemon, count badge, warning `StatusBadge` when `needsAttention`. Fixed height `--pi-group-header-height`; colours only from tokens; a landmark, not a button.
- `CardGrid` grouped mode (non-virtualised): for each header in `groups.window`, a `<section aria-label={label}>` with the header and a card grid of `cards.window.slice(start, start+count)` using the existing grid CSS (each group starts a new row; partial last row allowed). Flat mode unchanged.
- Empty state unchanged (grouping never hides cards).
- No path parsing, no ordering, no counting in TypeScript (golden rule 1): everything comes from the view.

## Out of scope
- Virtualised grouped layout (task-006). Collapse per group (deferred, W8).

## Acceptance criteria
- [ ] Flat mode: each card shows its workspace label with the full path as tooltip; a card without cwd shows "No workspace"; grid order unchanged.
- [ ] Grouped mode with three groups: three labelled sections in view order, counts shown, cards without the workspace line, the No-workspace section last (as sent).
- [ ] Header shows the daemon name only when cards from two daemons are present.
- [ ] `needsAttention` header shows the warning badge.
- [ ] Hostile label/path (HTML, very long) renders as clipped plain text.
- [ ] `npm run snapshot` in dark and light, flat and grouped (with a No-workspace group, a partial last row and an attention header), each image opened and checked (golden rule 5); record what was checked in the summary.

## Test / verification plan
- Component tests: `AgentCard.test.tsx`, new `WorkspaceHeader.test.tsx`, `CardGrid.test.tsx` (grouped branch), `App.monitor.test.tsx` if the sample view changes.
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test`
- `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` (both themes, both modes); look at every image.
- `cargo xtask ci` before done.

## Notes
- Card height stays `--pi-card-height`; adding the workspace to the meta line must not change it (the window maths assume a fixed height). Check in the snapshot that the meta line still fits one line.
