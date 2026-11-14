# Task 001 — Port the pi-studio design tokens and theme application — Summary

- **Sprint:** sprints/sprint-003-design-system
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
`src/tokens.css` is now pi-studio's `--pi-*` token set in the three blocks of the spec: theme-invariant scales in `:root` (spacing keys 0…128, font-size ladder `4xs…4xl` in rem, weights, radii, border widths, opacities, icon sizes, `--pi-font-ui/mono`, plus the dashboard-local `--pi-status-dot-size`, `--pi-card-min-width`, `--pi-card-edge-width`, `--pi-rail-width`, `--pi-statusbar-height`), dark colours + `--pi-shadow-sm/md/lg` in `:root, [data-theme="dark"]` (so a page with no `data-theme` is dark), light in `[data-theme="light"]`; both theme blocks declare the identical 26 names. No old name (`--chrome --card --field --control --text --text-dim --accent --danger --separator --radius`) survives anywhere; every stylesheet was rewritten against `--pi-*` (`app.css`, `App.module.css`, `MenuBar`, `StatusBar`, `Header`, `DaemonChips`, `CardGrid`, `AttentionRail`). `app.css` sets the base text to `font-size-xs`, `:focus-visible` to a 2 px `accent` outline offset 1 px, and carries the `prefers-reduced-motion` rule. The rail is `--pi-rail-width` on `surfaceSidebar`, the grid is `surfaceWorkspace`, the status bar follows § Status bar segments (35 px, `surface1`, hairline, one `surface2` segment with ellipsis + `title`).

`src/lib/theme.ts`: pure `resolveTheme(choice, prefersDark)`, `applyTheme(choice, doc, matchMedia)` (writes `data-theme`; for `system` registers exactly one `change` listener on `(prefers-color-scheme: dark)`; returns a cleanup that removes the listener and clears `data-theme`) and `useTheme(choice)` which `App` now uses instead of the inline effect. `useTheme(undefined)` (no view yet) writes nothing.

`tokens.test.ts` is rewritten per the spec (declared refs, colour literals, `color-mix` operands, dark/light parity, scale/theme disjointness, legal idents, no legacy names, value pin) and `tokens.pin.json` holds the pin with the pi-studio commit.

## accentForeground (spec TODO resolved)
pi-studio `contrastForeground(hex, dark="#18181b", light="#ffffff")` in `color-utils.ts` returns `dark` when the WCAG relative luminance is > 0.45, else `light`. Computed with the same formula: dark `#2e5cb8` → luminance 0.1170 → **`#ffffff`**; light `#253e6f` → luminance 0.0499 → **`#ffffff`**. Both pinned in `tokens.pin.json` and asserted in the test.
Values were read from pi-studio commit **`2581365399bf166157dc1a7dfdb6ee49df6fbac3`** (HEAD of `../pi-studio` when read; `packages/web-client/src/theme/{colors,variants,tokens,css-bridge}.ts` last changed in `aa1cf14`). Every colour, status colour and shadow in the spec table matched the sources; nothing was copied but values.

## Files created / changed
| File | Change |
|------|--------|
| `apps/pistudiodashboard/src/tokens.css` | rewritten (pi tokens) |
| `apps/pistudiodashboard/src/tokens.pin.json` | created (value pin + pi-studio commit) |
| `apps/pistudiodashboard/src/tokens.test.ts` | rewritten |
| `apps/pistudiodashboard/src/lib/theme.ts`, `theme.test.tsx` | created |
| `apps/pistudiodashboard/src/App.tsx` | `useTheme` replaces the inline effect |
| `apps/pistudiodashboard/src/App.test.tsx` | unset-before-view, `system` + live OS change tests |
| `apps/pistudiodashboard/src/app.css`, `App.module.css`, `components/{MenuBar,StatusBar,Header,DaemonChips,CardGrid,AttentionRail}.module.css` | rewritten onto `--pi-*` |
| `apps/pistudiodashboard/src/components/StatusBar.tsx` | single segment with `title` (text unchanged) |
| `swe/sprints/sprint-003-design-system/done/screenshots/s003-t001-*.png` | 4 PNGs |

## How it satisfies the scope
Follows design-system § What the existing template tokens.css becomes … § Token integrity check. Deviations: (1) the test also scans `.ts` (non-test) sources, a superset of the `.css/.tsx` the spec lists; (2) `color-mix` operands may also be `transparent` (the recipes use it, e.g. `color-mix(in srgb, <token> 18%, transparent)`); (3) hex values are compared case-insensitively (the spec table mixes case); (4) collapsed rail width uses `--pi-spacing-30` (30 px, was 28 px) because no 28 px token exists; (5) `applyTheme` clears `data-theme` on cleanup so an unmounted app leaves no stale attribute; (6) the status bar is still one segment (the totals string is one view label); the per-concern segments come with sprint-006.

## Build & test results
```
$ cd apps/pistudiodashboard && npm run check     # tsc --noEmit: clean
$ npm run lint                                   # oxlint --deny-warnings: clean
$ npm test                                       # 6 files, 47 tests passed, 0 failed
$ npm run build                                  # ok
# typo check by hand (not kept): appended `.x { color: var(--pi-color-sufrace0); }` to Header.module.css
$ npx vitest run src/tokens.test.ts   -> 1 failed | 16 passed: "components/Header.module.css: not declared in tokens.css: expected [ '--pi-color-sufrace0' ]"
# reverted -> 17 passed
```
`cargo xtask ci` is run once at the end of the sprint (task-003), not per task.

## Acceptance criteria
- [x] Token test: every `var(--pi-…)` declared, no colour literal outside `tokens.css`, dark/light same names (tokens.test.ts)
- [x] Value pin dark and light incl. computed `accentForeground` (tokens.test.ts "value pin")
- [x] `view.ui.theme` dark/light/system → `data-theme`; `system` follows a mocked media-query `change` with no command (App.test.tsx, theme.test.tsx)
- [x] No view yet → `data-theme` absent (App.test.tsx, theme.test.tsx); page is dark through the `:root` block
- [x] No old unprefixed token name in any `.css`/`.tsx` (tokens.test.ts "keeps none of the template's…")
- [x] Typo'd `var(--pi-color-sufrace0)` fails the token test (verified by hand, above; also a permanent unit test of the checker)

## Snapshots (rule 5)
Views = the real-daemon views saved by sprint-002/task-004 (`/tmp/shots/live-*.json`: the example shell fed a `feed.ingest` batch built from the live dev daemon's 7 agents; `empty-*.json` = `--empty`), drawn with `npm run snapshot -- --view … --size 960x560` in both themes. `done/screenshots/s003-t001-{live,empty}-{dark,light}.png`.
- Dark live: near-black chrome (`surface0`), cards lifted on `surface1` with a faint hairline, bold white titles, muted grey meta, one pill chip "workstation live 7", rail darker than the grid (`surfaceSidebar`) with the small spaced uppercase "NEEDS ATTENTION" header, status bar a 35 px strip with a rounded segment. No overflow, text readable.
- Light live: white grid, `#fafafa` cards with visible border, `#f4f4f5` rail clearly distinct from the white grid, dark text, muted meta readable; status-bar segment (`surface2` on `surface1`) is subtle but legible.
- Empty dark/light: centred "No daemons yet…" in muted text, no chip row, rail and status bar as above.
- Decision (spec TODO): the rail keeps **`surfaceSidebar`** (not `surface1`): in light it is `#f4f4f5` against the white `surfaceWorkspace`, which separates rail from grid even more clearly than the hairline alone; in dark it is darker than the grid.
- Not defects: cards have no status dot yet (task-002/sprint-006); "0 of 7 agents" is the D16 usage label.

## Smoke against pi-studio
The snapshots draw the 7 real agents of the dev daemon (127.0.0.1:6790, mock provider; statuses error ×1, running ×3, idle ×3, confirmed with `pi-studio ls`) through the real shell view. The app itself still has no client wiring (sprint-005); no window opened.

## Follow-ups / TODO(verify)
- Minimum WebKitGTK/WKWebView for `color-mix`/`@starting-style` stays with sprint-009 (task-001 itself adds none).
- Docs mention of the new token names belongs to the docs agent/task; not touched here.
