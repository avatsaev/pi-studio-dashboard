# Task 004 — Frontend cutover: monitor view types and layout skeleton — Summary

- **Sprint:** sprints/sprint-002-domain-cutover
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
`lib/types.ts` now mirrors the Rust view (`Theme` incl. `system`, the ten-field `UiState`, `Dialog`, `Toast`, `DaemonChip`, `StatusChip`, `Totals`, `Card`, `CardWindow`, `MonitorView`, `ShellCommandRow`, `View`; `DrillView = null` placeholder). The checklist (`Items*`, `hideDone`, `DocumentView`) is gone. `App` is a layout skeleton: `MenuBar`, `Header` ("Pi Studio Dashboard" + "N of M daemons connected"), `DaemonChips` (name, state word, agent count), `CardGrid` (region "Agents", the two empty-state texts, plain placeholder cards), `AttentionRail` (landmark "Attention rail", collapsed width from `attentionRailCollapsed`), `StatusBar` (connected text + `totals.label`, `ui.status` replaces it). `MenuBar` hides menus with no items and passes bound ids (`view.setTheme:light`) unchanged. `sampleView()`/`emptyView()` in `test-support.ts`. `crates/shell/examples/view.rs` gained `--empty`.

## Files created / changed
| File | Change |
|------|--------|
| `src/lib/types.ts`, `src/test-support.ts`, `src/App.tsx`, `src/App.module.css`, `src/App.test.tsx` | rewritten |
| `src/components/{Header,DaemonChips,CardGrid,AttentionRail}.tsx` + `.module.css` | created |
| `src/components/{StatusBar.tsx,StatusBar.test.tsx,MenuBar.tsx,MenuBar.test.tsx}` | rewritten / updated |
| `src/lib/shortcuts.test.ts`, `scripts/snapshot.mjs` (comments) | updated |
| `src/components/Items*` | deleted |
| `crates/shell/examples/view.rs` | `--empty` flag |
| `swe/sprints/sprint-002-domain-cutover/done/screenshots/` | 5 PNGs |

## Deviations
- Cards are drawn as placeholder `<article>`s (title, status word, daemon name, dimmed class) so a populated monitor is visible; the real card, chips behaviour and virtualisation stay in sprint-006.
- `StatusBar` imports `connectedText` from `Header` (the only counting in TS: the chips' own `live` states).
- `ui.theme === "system"` is written to `data-theme` unchanged (falls back to the dark `:root`); sprint-003/task-001 resolves it.
- Rust tests still mention `hideDone`/`item.add` only to assert they are refused/unknown; the TS tree and scripts contain none of the removed names.

## Build & test results
```
$ cd apps/pistudiodashboard && npm run check     # tsc: clean
$ npm run lint                                   # oxlint --deny-warnings: clean (after dropping a redundant role="region")
$ npm test                                       # 5 files, 21 tests passed (tokens.test.ts unchanged and passing)
$ npm run build                                  # ok
$ cargo xtask ci                                 # all 8 steps passed (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm)
```

## Acceptance criteria
- [x] Empty monitor: header, "No daemons yet…" empty state, rail landmark, status `0 of 0 daemons connected · 0 of 0 agents` (App.test)
- [x] `sampleView()`: both chips with state words (`live`, `backoff`); status shows the view's totals label (App.test, StatusBar.test)
- [x] `ui.status` replaces the summary and the summary returns when it clears (App.test)
- [x] `data-theme` follows dark then light (App.test)
- [x] `Ctrl+Shift+A` sends `invoke("view.toggleAttentionRail")`, rail unchanged locally (App.test)
- [x] Empty `Daemon`/`Agent` menus not drawn; `Theme: Light` sends `view.setTheme:light` (App.test, MenuBar.test)
- [x] No `hideDone`/`Items`/`DocumentView`/`item.add` in the frontend source; `tokens.test.ts` passes
- [x] Snapshots generated and looked at (below)

## Snapshots (rule 5)
Views came from the real shell `view` example: `live-*` = the example fed a `feed.ingest` batch (connState, hello, snapshot) built from the live pi-studio dev daemon's `list_agents_response` (7 agents: 3 running, 1 error, 3 idle); `empty-*` = `--empty`. Rendered with `npm run snapshot -- --view <json> --out … --size 960x560` (Playwright Chromium). Files in `done/screenshots/`.
- `s002-t004-live-dark.png`: menu bar with only "View"; header "Pi Studio Dashboard · 1 of 1 daemons connected"; one chip `workstation live 7`; a 2-column grid of 7 cards ordered error, running ×3, idle ×3 (title bold, "status · daemon" dim); right rail "Needs attention" (empty); status bar "1 of 1 daemons connected · 0 of 7 agents". No overflow or clipping; contrast of dim text acceptable.
- `s002-t004-live-light.png`: same layout; white cards on a light chrome, dark text, borders visible; readable.
- `s002-t004-empty-dark.png` / `-light.png`: header, no chip row (hidden when empty), centred "No daemons yet. Add one from the Daemon menu." in the grid area, rail with its heading, status `0 of 0 daemons connected · 0 of 0 agents`.
- `s002-t004-menu-dark.png` (`--click View`): popup with `✓ Theme: Dark`, `Theme: Light`, `Theme: System`, `Toggle Attention Rail  Ctrl+Shift+A`; exactly one check.
Observations, not defects of this task: "0 of 7 agents" is the usage-coverage label from the spec (D16), which reads oddly beside 7 visible cards until usage is fetched; the cards have no status dot yet (sprint-003/006); `system` theme not rendered (resolves in sprint-003).

## Smoke against pi-studio
Live agents (created with the pi-studio CLI) were fed through the shell's `feed.ingest` into the engine view and drawn by the real frontend (above). The app itself cannot connect yet (no client wiring, sprint-005); no window was opened.

## Follow-ups / TODO(verify)
- Docs still describe the old view/commands (task-005).
