# Task 006 — Virtualised grouped layout (UI) — Summary

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`src/lib/groupLayout.ts` (pure, no DOM): `groupOffsets` (a section is `headerHeight + rows * rowHeight`, sections one `gap` apart), `visibleGroupedRange` (first to last visible card index across sections, plus one row; a section whose title is visible counts its first row), `planGroups` (for the groups the shell sent and the card slice it sent: whole rows only, a partial row only at the end of its group, exact spacers for the rest, outer spacers). `CardGrid` now has one grouped branch for both modes: not virtualised draws everything; virtualised (`groups.layout` present) draws `planGroups` with spacers on the `.groups` container and inside each group's card grid, and reports `visibleGroupedRange` through the existing throttled `onWindow` (flat virtualised path untouched). Metrics now carry header height and gap (`--pi-group-header-height`, `--pi-spacing-12`); a change of `groupBy` scrolls to the top. A `GroupSection` component shares the section markup.
A defect found by the component test: a group whose only row is cut by the slice start produced a negative bottom spacer; one exact top spacer is used when no row is drawn (unit test added).
Tooling: `npm run snapshot` gained `--agents N` and `--scroll-to-slice`; the shell `view` example gained `--agents N` (documented in docs/development.md).

## Files changed
`apps/pistudiodashboard/src/lib/groupLayout.ts` (+ test), `src/components/CardGrid.tsx` (+ `CardGrid.grouped.test.tsx`), `scripts/snapshot.mjs`, `crates/shell/examples/view.rs`, `docs/development.md`.

## Build & test results
```
$ cd apps/pistudiodashboard && npm run check && npm run lint && npm test -> clean, 232 tests passed (214 before)
$ cargo xtask ci -> all 8 steps passed
```

## Snapshots, looked at
`done/screenshots/006-grouped-middle-{dark,light}.png`: 508 agents, 204 groups, slice at card 240 (grid scrolled to where the slice begins, so the 20-card overscan margin is on screen: the first section, project-085, shows its title over an empty spacer because its single row is only partly in the slice; in use that margin is off screen). Section titles, exact spacing between sections, card rows and the partial last row (3 cards over 2 columns) are right in both themes.

## Acceptance criteria
- [x] `groupOffsets`/`visibleGroupedRange` unit tests: one group = flat maths; many 1-card groups; a group taller than the viewport; top / middle / bottom; cols 1, 3, 5
- [x] 500 cards / 200 groups: stepping the scroll reports windows covering every card index; for 8 different slices the drawn spacers + sections add up exactly to the whole list height (no gap, no overlap)
- [x] Resize (columns 3 → 5) recomputes offsets and the reported range
- [x] Flat virtualised mode unchanged (its existing tests pass untouched)
- [x] Snapshot of a long grouped list scrolled to the middle, both themes, looked at

## Follow-ups / TODO(verify)
- Pixel exactness was proven by the height-sum test and screenshots, not in a real scroll with the platform webview (no display here).
