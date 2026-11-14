# Task 001 — Collapse and expand workspace groups

- **Sprint:** sprints/sprint-013-collapse-groups
- **Status:** done
- **Type:** feature
- **Area:** crates/engine (L5), crates/shell (L6), apps/pistudiodashboard/src (frontend)
- **Priority:** P2
- **Estimated size:** M
- **Depends on:** sprint-011 (done)

## Goal
Each workspace group can be collapsed to its title and expanded again; View menu Collapse All / Expand All.

## Scope references
- `swe/specs/features/agent-workspace-grouping.md` § Collapsing groups (W8)
- `crates/engine/src/view.rs`, `crates/shell/src/{state,fields,menus,control}.rs`, `apps/pistudiodashboard/src/{lib/groupLayout.ts,components/WorkspaceHeader.tsx,components/CardGrid.tsx}`

## Acceptance criteria
- [x] A collapsed group keeps its header, real count and attention badge; none of its cards are in the list; totals and chips unchanged
- [x] `view.toggleGroup`, `view.collapseAllGroups`, `view.expandAllGroups` with reasons when they cannot run; `ui.set {collapsedGroups}` validated and bounded
- [x] The header is a button with `aria-expanded`
- [x] Virtualised grouped lists keep exact heights and report card indices with collapsed groups in them
- [x] With every group collapsed the titles are shown, not the empty state
- [x] Collapse state is session state (not saved)

## Test / verification plan
`cargo test` (engine, shell), `npm run check && npm run lint && npm test`, `npm run snapshot`, smoke against the dev daemon, `cargo xtask ci`.
