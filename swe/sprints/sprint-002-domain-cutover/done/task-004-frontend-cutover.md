# Task 004 — Frontend cutover: monitor view types and layout skeleton

- **Sprint:** sprints/sprint-002-domain-cutover
- **Status:** done
- **Type:** feature
- **Area:** frontend `apps/pistudiodashboard/src`
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-003

## Goal
Make the webview draw the new view: `lib/types.ts` mirrors the Rust view, the sample checklist components are deleted, and `App` renders a layout skeleton (header with app name, daemon-chip row, card-grid area with an empty state, right-rail area, status bar) with no business logic.

## Context / why
After tasks 002–003 the shell sends the monitor view and the new `UiState`; the frontend still draws the checklist (`Items`, `hideDone`). This task closes the vertical cut so the running app shows an honest empty monitor, and fixes the type contract that sprints 003–010 build on (rule 1: the frontend only draws the view and sends command ids; rule 5: look at a rendered image).

## Scope references
- `swe/specs/features/agent-monitor.md` § View shape (card/chip/totals/drill types), § Card grid (empty state wording is defined there — read it), § Status dot mapping (only the `DotKind` type is needed here)
- `swe/specs/architecture/app-structure.md` § Webview boundary, § UiState (the TS mirror), § Command registry (menu shape)
- `swe/specs/architecture/design-system.md` § Theme switching (note: `system` handling is sprint-003/task-001)
- Files (real, current): `apps/pistudiodashboard/src/{App.tsx,App.module.css,App.test.tsx,test-support.ts,tokens.css,tokens.test.ts,main.tsx}`, `src/lib/{types.ts,backend.ts,shortcuts.ts}`, `src/components/{Items.tsx,Items.module.css,Items.test.tsx,MenuBar.tsx,MenuBar.module.css,MenuBar.test.tsx,StatusBar.tsx,StatusBar.module.css}`, `apps/pistudiodashboard/scripts/snapshot.mjs`, `crates/shell/examples/view.rs`
- Create: `src/components/{Header,DaemonChips,CardGrid,AttentionRail}.tsx` (+ CSS Modules) — names may be adjusted if a smaller split reads better; one component per screen region.

## What to build
- **`lib/types.ts`** (mirrors `control::view` and `document.inspect`; Rust is the source of truth, comments say so):
  - `Theme = "dark" | "light" | "system"`; `UiState { theme, filterDaemons, filterStatuses, attentionRailCollapsed, focusItem, gridWindow, dialog, diagnosticsTarget, toasts, status }` with `Dialog` (tagged union, `kind`), `FocusItem`, `GridWindow`, `Toast { id, kind: "info"|"warning"|"error", text }`;
  - `AgentStatus`, `DotKind`, `ConnState`, `DaemonChip`, `StatusChip`, `Totals`, `Card`, `CardWindow`, `DrillView` (typed as `null` payload for now — filled in sprint-006/task-006, keep the field), `MonitorView` (the `document` value), `View { ui, document: MonitorView | null, menus, shellCommands }`. Remove `Item`, `DocumentView`, `hideDone`. `u64` ids/timestamps are JSON numbers (exact below 2^53) — keep that comment where it applies.
- **Delete** `components/Items.tsx`, `Items.module.css`, `Items.test.tsx`.
- **`App.tsx` layout skeleton** — one function of the single `view` value (unchanged `onView`/`ui.inspect` plumbing, shortcut handling and `data-theme` effect stay; `hideDone` references go). Regions, each a labelled landmark so tests and the snapshot read cleanly: `<header>` with the app name "Pi Studio Dashboard" (and the daemon connected count text); the daemon-chip row drawn from `document.daemonChips` (name + state word; display only — chip filtering is sprint-006/task-004); the card-grid area (`role="region"`, `aria-label="Agents"`) with an empty state when `cards.total === 0` (text per agent-monitor § Card grid: no daemons configured vs. daemons but no agents; if the spec has no wording, use "No daemons yet. Add one from the Daemon menu." / "No agents on the connected daemons."); the right-rail area (`aria-label="Attention rail"`, collapsed width when `railCollapsed`; content empty in this task); the status bar. No daemon/agent logic, no formatting of domain data beyond labels.
- **`StatusBar.tsx`**: segment text from `document.totals.label` and daemon chip counts (e.g. `"0 of 0 daemons connected"`, `"0 of 0 agents"`); `ui.status` error still replaces the summary; no "edited"/"left" logic.
- **`MenuBar.tsx`**: unchanged behaviour (draws generated menus, passes ids unchanged — including bound ids like `view.setTheme:light`); menus with no items are not drawn (Daemon/Agent are empty until sprints 005–007).
- **Styles**: use the **existing** token names in `tokens.css` (the `--pi-*` replacement is sprint-003/task-001); no colour literals (`tokens.test.ts` stays as is and must pass). `ui.theme === "system"` is written to `data-theme` unchanged for now and falls back to the dark `:root` block — documented; sprint-003/task-001 resolves it.
- **`test-support.ts`**: `sampleView()` returns a realistic `View`: two daemon chips (one live, one in backoff with `staleSince`), a few cards in different statuses incl. an attention card, totals, the new `ui` defaults, menus `Daemon`, `Agent` (empty) and `View` (`Theme: Dark/Light/System` with bound ids, `Toggle Attention Rail` with shortcut `Cmd+Shift+A`), shell commands. Add `emptyView()` for the empty-monitor state. Keep `fakeBackend`.
- **Tests (vitest + testing-library, fake backend):** `App.test.tsx` rewritten (draws header/chips/empty state/status bar from the view; a status error replaces the summary; `data-theme` follows `ui.theme` for dark/light; a shortcut becomes `invoke("view.toggleAttentionRail")`), `StatusBar` test, `MenuBar.test.tsx` updated (empty menus hidden, bound id passed through). Delete tests of removed behaviour; no new tests for presentational CSS.
- **Snapshot:** `crates/shell/examples/view.rs` can emit a monitor view with a couple of daemons/agents (task-002); run `npm run snapshot` for dark and light (`--script '[["ui.set", {"theme":"dark"}]]` / light) and, with `--view <file>` if needed, once for the empty monitor. Look at all images (rule 5) and record in the task summary what was verified; fix visible defects (overflow, contrast) before done.

## Out of scope
- `--pi-*` tokens, theme resolution for `system`, primitive components (StatusDot, FilterChip, Toast viewport, Dialog, …) → sprint-003.
- Card rendering, filter chips behaviour, virtualisation → sprint-006/task-004; rail items → sprint-007/task-005; drill-down → sprint-006/task-006; dialogs → sprint-005/task-005.
- Docs → task-005.

## Acceptance criteria
- [ ] Given the empty monitor view, when the app renders, then the header shows "Pi Studio Dashboard", the grid area shows the "no daemons" empty state, the rail landmark exists and the status bar shows `0 of 0 agents`.
- [ ] Given `sampleView()`, then the daemon-chip row shows both daemons with their state words, and the status bar shows the totals label from the view (no recomputation in TypeScript).
- [ ] Given a view whose `ui.status` is non-empty, then the status bar shows that text instead of the summary; when the next view clears it, the summary returns.
- [ ] Given `ui.theme` `dark` then `light`, then `<html data-theme>` is `dark` then `light`.
- [ ] Given the `Cmd/Ctrl+Shift+A` key combination, then the backend receives `invoke("view.toggleAttentionRail")` and the frontend changes nothing locally.
- [ ] Given the menus in the view, then `Daemon` and `Agent` (no items) are not drawn, and clicking `Theme: Light` sends the bound id exactly as given by the shell.
- [ ] Given the source tree, then no file mentions `hideDone`, `Items`, `DocumentView` or `item.add`, and `tokens.test.ts` passes (no colour literal added).
- [ ] The snapshot images (dark, light, empty) were generated and looked at; any defect fixed.

## Test / verification plan
- Typecheck/Lint/Tests: `cd apps/pistudiodashboard && npm run check && npm run lint && npm test`.
- UI: `npm run snapshot -- --out dark.png` (and light, empty) — look at the images; keep them out of git unless the repo tracks snapshots (check `.gitignore`).
- Gate: `cargo xtask ci` passes (its `web` step runs the frontend checks).

## Notes
- Between sprint-002/task-002 and this task the running app would draw a stale checklist shape; this task removes that gap. If `npm run snapshot` needs `npm run snapshot:install` (Playwright Chromium), run it once.
- WebKitGTK/WKWebView differ from Playwright's Chromium; platform quirks are checked later in the real app (sprint-009).
- Keep component files small and one-region-each so sprints 003–008 replace them in place rather than rewriting `App.tsx`.
