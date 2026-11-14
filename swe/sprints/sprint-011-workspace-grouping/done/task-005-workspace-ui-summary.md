# Task 005 — Workspace on cards and grouped sections (UI) — Summary

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
Frontend: types for `Card.cwd/workspaceLabel`, `GroupBy`, `GroupHeader`, `GroupsView`, `MonitorView.groupBy/groups`, `UiState.groupBy`. `AgentCard` shows the workspace label after the daemon name (full path as tooltip; `showWorkspace={false}` in grouped mode). New `WorkspaceHeader` (label, muted path, daemon name only when cards of several daemons are shown, "needs attention" warning badge, count badge; fixed height `--pi-group-header-height: 32px`, added to `tokens.css` and `tokens.pin.json`). `CardGrid` draws, with `groupBy: "workspace"` and ≤ 100 cards, one `<section aria-label=label>` per group from `groups.window` and `cards.window` (slice by `start - cards.start`); flat mode unchanged. The virtualised grouped branch is task-006 (until then a virtualised grouped view falls back to the flat renderer). No path parsing/ordering/counting in TypeScript. The shell `view` example now seeds workspaces (shared folder name, a pending dialog, agents without cwd) so snapshots can show grouping.

## Files changed
`apps/pistudiodashboard/src/lib/types.ts`, `components/{AgentCard,CardGrid}.tsx|css`, new `components/WorkspaceHeader.tsx|css`, `tokens.css`, `tokens.pin.json`, `test-support.ts`, `test-support-monitor.ts` (`groupedOf`, `groupedView`), tests: `AgentCard.test.tsx`, `WorkspaceHeader.test.tsx`, `CardGrid.grouped.test.tsx`, `MenuBar.test.tsx` (find the item by id, the View menu gained entries); `crates/shell/examples/view.rs`.

## Build & test results
```
$ cd apps/pistudiodashboard && npm run check && npm run lint && npm test -> clean, 214 tests passed (was 204)
$ cargo xtask ci -> all 8 steps passed
```

## Snapshots (golden rule 5), looked at
`done/screenshots/005-{flat,grouped}-{dark,light}.png` from `npm run snapshot` over the shell example: flat cards show `workstation · a/app · mock-1` on one line, card height unchanged; grouped shows sections `a/app` (error → "needs attention", 3), `b/app` (pending dialog → "needs attention", 2), `docs` (1), a partial last row, headers with muted full path and count badge; both themes readable, warning badge in the warning tone, header underline in the border token. Not covered by a snapshot: the daemon name in headers (two daemons; unit-tested) and the "No workspace" section (below the fold in the image; covered by tests and by the flat card text "No workspace").

## Acceptance criteria
- [x] Flat: workspace label + path tooltip, "No workspace", order unchanged
- [x] Grouped: sections in sent order, counts, no workspace line on cards, No workspace last as sent
- [x] Header daemon name only with several daemons
- [x] needsAttention warning badge
- [x] Hostile label/path rendered as clipped plain text (`WorkspaceHeader.test.tsx`)
- [x] Snapshots both themes, both modes, looked at

## Follow-ups / TODO(verify)
- Task 006 replaces the temporary flat fallback for virtualised grouped views.
