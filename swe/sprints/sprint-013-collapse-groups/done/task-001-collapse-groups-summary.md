# Task 001 — Collapse and expand workspace groups — Summary

- **Sprint:** sprints/sprint-013-collapse-groups
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
- Engine (`view.rs`): `document.inspect` takes `collapsedGroups` (keys) and `includeGroupKeys`. A collapsed group's cards are left out of the card list (so windowing and virtualisation follow what is drawn); its header carries `index`, `count` (real), `shown` (0 when collapsed), `collapsed`; `layout` holds `shown`; a zero-card group belongs to a slice when its list position is inside it.
- Shell: `UiState.collapsedGroups` (bounded: 5,000 distinct keys of 1–1,024 characters, validated for `ui.set` and the command), commands `view.toggleGroup {key}` (no menu), `view.collapseAllGroups`, `view.expandAllGroups` (View menu; disabled with reasons when not grouped, or nothing collapsed). `control::inspect_params` is now the one place the view parameters are built.
- Frontend: `WorkspaceHeader` is a button (`aria-expanded`, chevron, focus ring); a collapsed section is its title only; the empty state is not shown when only collapsed titles remain; `groupLayout.ts` handles 0-card groups (`planGroups` takes `index`-based consecutive runs, `visibleGroupedRange` asks for the card index where a screen of titles sits).
- Spec § Collapsing groups, `docs/control-protocol.md`, PLAN (sprint 013).

## Build & test results
```
$ cargo test (engine, shell) -> passed (4 new engine tests, 5 new shell tests)
$ npm run check && npm run lint && npm test -> clean, all passed (new: header button, collapsed sections, all-collapsed, exact heights with collapsed groups in a virtualised list, range for collapsed titles; 7 layout tests)
$ cargo xtask ci -> see the final result in the chat
```

## Smoke against pi-studio
Headless app against the dev daemon on 6790: 25 agents in 7 groups; `view.toggleGroup` on the second group → 23 cards, that group `shown 0, collapsed true, count 2`; `view.collapseAllGroups` → 0 cards, 7 headers all collapsed; `view.expandAllGroups` → 25 cards; a second `expandAllGroups` → `no group is collapsed`; after `view.setGroupBy none`, `view.collapseAllGroups` → `the agents are not grouped by workspace`. The config file has no `collapsedGroups` (session state).

## Snapshots, looked at
`done/screenshots/collapsed-{dark,light}.png`: all groups collapsed except `b/app`; collapsed headers show a right chevron, count and the "needs attention" badge; the open one shows a down chevron and its cards; both themes fine.

## Not verified
Mouse/keyboard behaviour in a real window (no display); the button is a real `<button>` (Enter/Space work natively), covered by the component test's click only.
