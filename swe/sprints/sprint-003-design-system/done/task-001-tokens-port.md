# Task 001 — Port the pi-studio design tokens and theme application

- **Sprint:** sprints/sprint-003-design-system
- **Status:** done
- **Type:** feature
- **Area:** frontend `apps/pistudiodashboard/src` (tokens, theme)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** sprint-002-domain-cutover/task-004

## Goal
Replace the template `tokens.css` with pi-studio's `--pi-*` colour and scale tokens (dark + light), rewrite every existing stylesheet against them (no aliases), apply the theme choice to `<html data-theme>` (resolving `system` in the webview), and turn `tokens.test.ts` into the token-integrity and value-pin check.

## Context / why
D20/D21: the dashboard reuses pi-studio's design language by porting token *values* (not source) so both products look related, in dark (default), light and follow-system. A token typo must never silently fall back to a wrong colour, and the ported values must be provably those of pi-studio (brief acceptance 14). This is a clean cutover: nothing keeps the old unprefixed names.

## Scope references
- `swe/specs/architecture/design-system.md` § What the existing template `tokens.css` becomes, § Colour tokens, § Scale tokens (incl. dashboard-local additions), § Theme switching, § Theme application, § Token integrity check, § Error handling & edge cases (theme rows), § Acceptance criteria (token/theme items), § TODO(verify)
- `swe/specs/architecture/app-structure.md` § UiState (`theme`)
- `swe/grills/daemon-monitoring-dashboard.md` D20, D21
- Real files: `apps/pistudiodashboard/src/{tokens.css,tokens.test.ts,app.css,App.tsx,App.module.css,main.tsx}`, `src/components/{MenuBar,StatusBar,Header,DaemonChips,CardGrid,AttentionRail}.module.css` (as created/kept by sprint-002/task-004)
- Reference values (read-only, never copied): `../pi-studio/packages/web-client/src/theme/{colors,variants,tokens,css-bridge,color-utils}.ts`, `../pi-studio/packages/web-client/DESIGN_SYSTEM.md`, pi-studio `token-integrity.test.ts`
- Create: `src/lib/theme.ts` (+ `theme.test.ts`), optional `src/tokens.pin.json`

## What to build
- **`src/tokens.css`** in three blocks exactly as the spec: (1) `:root { … }` theme-invariant scales — spacing `--pi-spacing-<px>` for keys `0 1 2 3 4 5 6 7 8 10 11 12 14 16 20 24 26 30 32 48 64 80 96 128`, font sizes `4xs…4xl` (rem), weights, radii, border widths, opacities, icon sizes, `--pi-font-ui`/`--pi-font-mono`, and the dashboard-local `--pi-status-dot-size` (8px), `--pi-card-min-width` (280px), `--pi-card-edge-width` (2px), `--pi-rail-width` (320px), `--pi-statusbar-height` (35px); (2) `:root, [data-theme="dark"] { color-scheme: dark; … }` with every colour token of the spec's table (dark column) plus `--pi-shadow-sm/md/lg`; (3) `[data-theme="light"] { color-scheme: light; … }` with the light column. Blocks 2 and 3 declare identical token-name sets. Colour names are `--pi-color-<camelCase>` verbatim (e.g. `--pi-color-surfaceSidebarHover`). `--pi-color-scheme` is not emitted. No bundled fonts.
- **`accentForeground`**: resolve the spec's TODO — read pi-studio `contrastForeground` in `color-utils.ts`, compute its result for `#2e5cb8` (dark) and `#253e6f` (light), pin the real hex values (do not guess; white is expected). Record both in the pin file and the task summary.
- **Rewrite all stylesheets** to the `--pi-*` names: `app.css`, `App.module.css`, `MenuBar.module.css`, `StatusBar.module.css` and the region components from sprint-002/task-004. Base text `font-size: var(--pi-font-size-xs)`; section headers `3xs` uppercase bold `letter-spacing: 0.1em`; `StatusBar` segments per design-system § Status bar segments (height `--pi-statusbar-height`, `surface1`, hairline `--pi-color-border`); rail width `--pi-rail-width` on `--pi-color-surfaceSidebar`; grid area `surfaceWorkspace`. `:focus-visible` shows a 2px `--pi-color-accent` outline offset 1px on interactive elements. `@media (prefers-reduced-motion: reduce)` rule set up for later animations. Remove every old token name (`--chrome`, `--card`, `--field`, `--control`, `--text`, `--text-dim`, `--accent`, `--danger`, `--separator`, `--radius`).
- **Theme application (`lib/theme.ts`):** pure `resolveTheme(choice: Theme, prefersDark: boolean): "dark" | "light"`; `applyTheme(choice, doc, mql)` / a `useTheme(choice)` hook used by `App.tsx` replacing the inline effect: writes `document.documentElement.dataset.theme = resolved`; for `system` registers exactly one `matchMedia("(prefers-color-scheme: dark)")` `change` listener that re-resolves, and removes it for any other choice or on unmount. Before the first `view` arrives `data-theme` is unset (dark via `:root`, no light flash).
- **`tokens.test.ts` rewritten** per spec § Token integrity check: (a) every `var(--pi-…)` reference in any `.css`/`.tsx` under `src` (excluding `tokens.css` and `*.test.tsx`) is declared in `tokens.css`; (b) no colour literal (`#hex`, `rgb(a)`, `hsl(a)`, `oklch`, `oklab`, `lab`, `lch`, `color()`) outside `tokens.css` (kept from the scaffold) and `color-mix(in srgb, var(--pi-…) …)` uses only `var()` operands; (c) dark and light blocks declare the same token names (replaces "exactly two blocks"); (d) the `:root` scale names are disjoint from the theme colour names; (e) no illegal custom-property ident (no `.` in spacing keys); (f) value pin: every colour/scale token in the spec tables equals its pinned value for dark and light (pin data in the test or `src/tokens.pin.json`, with the pi-studio commit it was read at — record the commit hash, TODO(verify) in the spec).
- **`theme.test.ts`:** `resolveTheme` table; hook/function test with a mocked `matchMedia` proving `system` follows the media query live and that the listener is removed when the choice changes.

## Out of scope
- Primitive components (StatusDot, badges, toasts, dialogs, …) → task-002.
- `ATTRIBUTION.md` provenance row and snapshots → task-003. (Add the row in the same change as `tokens.css` if you are working strictly by rule 9 — the row text is defined in task-003; it is acceptable to land both together, then task-003 only verifies/records.)
- Card, rail-item, drill-down styling (their sprints).
- Minimum WebKitGTK/WKWebView version for `color-mix`/`@starting-style` (spec TODO(verify)) — checked in the real app in sprint-009; note any `color-mix` usage with a solid fallback only where task-002 needs it.

## Acceptance criteria
- [ ] Given `tokens.css`, when the token test runs, then every `var(--pi-…)` used anywhere in `src` is declared, no colour literal exists outside `tokens.css`, and the dark and light blocks declare the same names. (design-system AC 1; D20, D21)
- [ ] Given the value pin, then each dark and light colour/scale token equals the table value (including the pinned `accentForeground` computed from pi-studio's function). (AC 2; brief acceptance 14)
- [ ] Given `view.ui.theme` `dark`, `light`, `system`, when the view arrives, then `<html data-theme>` is `dark`, `light`, and the media-query result respectively; with `system`, an OS appearance change (mock `change` event) re-applies without any command. (AC 3; D21)
- [ ] Given no view yet, then `data-theme` is absent and the page is dark (`:root` block).
- [ ] Given the source tree, then none of the old unprefixed token names remains in any `.css`/`.tsx`.
- [ ] Given a typo'd `var(--pi-color-sufrace0)` introduced in any stylesheet, then the token test fails (verify once by hand, do not commit the typo).

## Test / verification plan
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test`.
- UI: `npm run snapshot -- --out dark.png --script '[["ui.set", {"theme":"dark"}]]'` and the same for `light`; look at both (rule 5) — the skeleton must remain legible in both.
- Gate: `cargo xtask ci` passes.

## Notes
- TODO(verify) (design-system.md): `accentForeground` hex — resolved here; light-theme `surfaceWorkspace`/`surfaceSidebar` vs `surface1` for the rail — confirm against the first snapshot and note the decision; the pi-studio commit hash for the pin — record (pi-studio HEAD at planning was `47d80e6`; use the commit actually read).
- `system` is verified by the unit test with mocked `matchMedia`, not by snapshot (spec § Snapshot verification).
- Pi-studio font ladder: rem against an untouched 16 px root; do not set `html { font-size }`.
