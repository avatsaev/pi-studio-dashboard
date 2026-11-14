# Architecture — Design System

> Part of: [overview.md](../overview.md)
> Dependencies: [app-structure.md](app-structure.md) (UiState `theme`, webview boundary, snapshot flow)
> Source brief: [daemon-monitoring-dashboard.md](../../grills/daemon-monitoring-dashboard.md) (D20, D21, D11, D15, D26, D27, D32; acceptance criterion 14)

## Purpose

Reuse pi-studio's design language in the dashboard frontend without copying source: port the token **values** into `apps/pistudiodashboard/src/tokens.css` under pi-studio's `--pi-*` names, and rebuild the few components needed from pi-studio's recipes (D20). Themes: dark (default), light, follow-system (D21). Colour is reserved for state: the look is dense, IDE-like, neutral surfaces with status colour only where it carries meaning.

Reference (read-only, never copied): `pi-studio/packages/web-client/DESIGN_SYSTEM.md`, `src/theme/{colors,variants,tokens,css-bridge}.ts`, `ui/status-dot.ts`, `components/primitives/{StatusDot,StatusBadge,ToastViewport}*`, `features/sessions/{status-map.ts,SessionList.module.css}`, `features/chat/rows/rows.module.css` (tool cards), `features/workspace/StatusBar.module.css`.

## Public contract

### What the existing template `tokens.css` becomes

The scaffold defines unprefixed tokens in two blocks (`--chrome --card --field --control --text --text-dim --accent --danger --separator --radius`, dark in `:root,[data-theme="dark"]`, light in `[data-theme="light"]`) and `tokens.test.ts` enforces (a) no colour literal outside `tokens.css` in any `.css`/`.tsx` and (b) exactly two blocks with identical token names. **All template tokens are removed** and every component/style using them (`App.module.css`, `MenuBar`, `StatusBar`, `Items*`) is rewritten against the `--pi-*` names below; nothing aliases the old names (clean cutover). The colour-literal check is kept. The block-parity check is changed (see Token integrity).

File layout of the new `tokens.css`:
1. `:root { … }` — theme-invariant scales (spacing, font sizes, radii, border widths, opacity, icon sizes, weights, fonts).
2. `:root, [data-theme="dark"] { color-scheme: dark; … }` — dark colours + dark shadows.
3. `[data-theme="light"] { color-scheme: light; … }` — light colours + light shadows.

Blocks 2 and 3 declare the identical token-name set.

### Colour tokens (values ported from pi-studio's `dark` and `light` variants)

Names are emitted verbatim camelCase after `--pi-color-`, as pi-studio's css-bridge does.

| Token | Dark | Light | Use |
|---|---|---|---|
| `--pi-color-surface0` | `#181B1A` | `#ffffff` | App background, list-row hover |
| `--pi-color-surface1` | `#1E2120` | `#fafafa` | Cards, panels, status bar, toast, tool/stream cards |
| `--pi-color-surface2` | `#272A29` | `#f4f4f5` | Inputs, chips at rest, status-bar segments, popovers |
| `--pi-color-surface3` | `#434645` | `#e4e4e7` | Disc/avatars, strong separators |
| `--pi-color-surface4` | `#595B5B` | `#d4d4d8` | Scroll thumbs fallback, deepest emphasis |
| `--pi-color-surfaceSidebar` | `#141716` | `#f4f4f5` | Attention rail / right panel background |
| `--pi-color-surfaceSidebarHover` | `#1c1f1e` | `#e9e9ec` | Rail item hover |
| `--pi-color-surfaceWorkspace` | `#1E2120` | `#ffffff` | Card-grid area background |
| `--pi-color-foreground` | `#fafafa` | `#1a1a1e` | Primary text |
| `--pi-color-foregroundMuted` | `#A1A5A4` | `#71717a` | Secondary text, idle dot, meta |
| `--pi-color-accent` | `#2e5cb8` | `#253e6f` | Selection, running state, focus ring |
| `--pi-color-accentBright` | `#a2b4d7` | `#2f4f8e` | Running spinner ring, links, emphasised accent text |
| `--pi-color-accentForeground` | contrast of `accent` (pi-studio computes it with `contrastForeground`; see TODO) | same rule | Text on solid accent |
| `--pi-color-destructive` | `#c64f43` | `#b04138` | Destructive buttons (Interrupt confirm), failed-turn tint |
| `--pi-color-destructiveForeground` | `#ffffff` | `#ffffff` | Text on destructive |
| `--pi-color-border` | `#252B2A` | `#e4e4e7` | Hairlines, card outlines |
| `--pi-color-borderAccent` | `#2f3135` | `#ececf1` | Emphasised hairline (hover outline) |
| `--pi-color-scrollbarHandle` | `#717574` | `#3f3f46` | Scrollbar thumb |
| `--pi-color-statusSuccess` | `#16a34a` | `#15803d` | Healthy / completed |
| `--pi-color-statusDanger` | `#dc2626` | `#b91c1c` | Error, failed turn, auth failed |
| `--pi-color-statusWarning` | `#f59e0b` | `#d97706` | Needs attention (pending dialog), stale, degraded, "unencrypted" |
| `--pi-color-statusInfo` | `#3b82f6` | `#2563eb` | Informational badges (version, tool kind) |
| `--pi-color-statusMerged` | `#9333ea` | `#7c3aed` | Not used in v1; kept for token parity only if a component needs it |

Not ported (no consumer): syntax/terminal colours, diff colours, `surfaceDiffEmpty`, legacy alias tokens (`background`, `popover`, `primary`, `muted`, `input`, `ring`, …), and the four other dark variants (D21).

`--pi-color-scheme` is not emitted; `color-scheme` is a CSS property in each theme block.

### Scale tokens (theme-invariant, `:root`)

| Group | Tokens | Values |
|---|---|---|
| Spacing | `--pi-spacing-<px>` | keys `0 1 2 3 4 5 6 7 8 10 11 12 14 16 20 24 26 30 32 48 64 80 96 128` → that many `px` (keys are literal px values, dot-free, as in pi-studio) |
| Font size (rem against the untouched 16 px root) | `--pi-font-size-<key>` | `4xs` 0.625rem (10), `3xs` 0.6875rem (11), `2xs` 0.75rem (12), `xs` 0.8125rem (13), `code` 0.8125rem (13), `sm` 0.875rem (14), `md` 0.9375rem (15), `base` 1.0625rem (17), `lg` 1.1875rem (19), `xl` 1.3125rem (21), `2xl` 1.4375rem (23), `3xl` 1.75rem (28), `4xl` 2.25rem (36) |
| Font weight | `--pi-font-weight-<key>` | `normal` 400, `medium` 500, `semibold` 600, `bold` 700 |
| Radius | `--pi-radius-<key>` | `none` 0, `sm` 2px, `base` 4px, `md` 6px, `lg` 8px, `xl` 12px, `2xl` 16px, `full` 9999px |
| Border width | `--pi-border-width-<key>` | `0` 0px, `1` 1px, `2` 2px |
| Opacity | `--pi-opacity-<key>` | `0` 0, `50` 0.5, `100` 1 |
| Icon size | `--pi-icon-size-<key>` | `xs` 12px, `sm` 14px, `md` 16px, `lg` 20px |
| Fonts | `--pi-font-ui`, `--pi-font-mono` | UI: `system-ui, -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif`; mono: `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace`. No bundled fonts (the dashboard ships none; `ATTRIBUTION.md` already says so) |
| Shadows (per theme block) | `--pi-shadow-sm/md/lg` | `0 1px 2px / 0 3px 8px / 0 10px 24px` with colour `rgba(0,0,0,0.55)` in dark and `rgba(24,24,27,0.12)` in light |

Dashboard-local additions (named in the same `--pi-` namespace, not in pi-studio; each declared in all relevant blocks):

| Token | Value | Use |
|---|---|---|
| `--pi-status-dot-size` | `8px` | Status dot (pi-studio `STATUS_DOT_SIZE`) |
| `--pi-card-min-width` | `280px` | Card grid column minimum |
| `--pi-card-edge-width` | `2px` | Card edge bar |
| `--pi-rail-width` | `320px` | Attention rail / drill-down panel width |
| `--pi-statusbar-height` | `35px` | Status bar (pi-studio value) |

Base text: `font-size: var(--pi-font-size-xs)` (13 px) for dense UI, `sm` for card titles, `3xs` uppercase bold with `letter-spacing: 0.1em` for section headers (pi-studio `SessionList` header recipe). No Tailwind, no CSS-in-JS: CSS Modules + `tokens.css`.

### Theme switching

- `UiState.theme` carries the user's **choice**: `"dark" | "light" | "system"` (default `dark`, D21; see [app-structure.md](app-structure.md)), set by `view.setTheme` and persisted in the config settings.
- The frontend writes `data-theme="dark"|"light"` on `<html>`. For `"system"` the **webview resolves** the choice with `window.matchMedia("(prefers-color-scheme: dark)")`, subscribes to its `change` event, and re-applies. Justification: the resolved value is display-only (no command, model or daemon behaviour depends on it), the media query is the only reliable source of the OS appearance inside the webview on both Linux (WebKitGTK) and macOS, and resolving in Rust would require a per-platform OS-appearance hook for no behavioural gain. The view therefore carries the choice, not the resolution; automation sees `"system"` and the snapshot tool passes the theme explicitly.
- `data-theme` absent (before first `view`) defaults to dark via the `:root` selector, so there is no light flash on a dark system at startup.

### Components to rebuild

All are dashboard frontend components reading the view; none holds daemon state. Recipes use tokens only (colour literals are forbidden outside `tokens.css`).

**Status mapping** (the dashboard's single translation point, replacing pi-studio `status-map.ts` + `status-dot.ts`): daemon `AgentStatus` → dot.

| Status / flag | Dot | Notes |
|---|---|---|
| `running` | spinning ring: `border-top/right` `--pi-color-accentBright` | not a flat dot, so it reads as work in progress (pi-studio recipe) |
| `initializing` | flat dot `--pi-color-accent` | pi-studio maps `initializing`→`queued`→accent |
| `idle` | flat dot `--pi-color-foregroundMuted` | dashboard always shows inactive dots (pi-studio hides them unless `showInactive`) |
| `error` | `--pi-color-statusDanger` | |
| `closed` | `--pi-color-foregroundMuted`, card at `--pi-opacity-50` | Dashboard decision: D15 dims closed agents; pi-studio would map `closed`→`finished`→success green, which is deliberately **not** reused because a dimmed green dot reads as "healthy" |
| needs attention: pending dialog (`agent_ui_request`, `expectsResponse`) | `--pi-color-statusWarning`, optional pulse ring | attention overrides base status colour (pi-studio rule) |
| failed turn flag | `--pi-color-statusDanger` | |
| unknown value (D26) | outlined dot `--pi-color-foregroundMuted`, label "unknown" | |

No dot is ever colour-only: it carries an `aria-label` with the status word, and the card shows a text label as well.

| Component | Recipe |
|---|---|
| **Status dot** | 8 px circle (`--pi-status-dot-size`), colour per table; running = spinner (`@keyframes` rotation; disabled under `prefers-reduced-motion`); `role="img"` + `aria-label` |
| **Status badge** (tinted pill) | `padding: --pi-spacing-1 --pi-spacing-6`; `radius-full`; `1px solid`; `font-size-xs`, weight 500; background `color-mix(in srgb, <token> 18%, transparent)`, border `color-mix(in srgb, <token> 45%, transparent)`, text colour `<token>` — `<token>` ∈ `statusSuccess|statusDanger|statusWarning|statusInfo|accent|foregroundMuted`. Used for: daemon version, "unencrypted", "duplicate entry", degraded flags, client type |
| **Agent card** (no pi-studio precedent; defined here) | Grid cell `min-width: --pi-card-min-width`, grid `repeat(auto-fill, minmax(var(--pi-card-min-width), 1fr))`, gap `--pi-spacing-12`. Surface `--pi-color-surface1`, `1px` `--pi-color-border`, radius `--pi-radius-lg`, padding `--pi-spacing-10 --pi-spacing-12`. Left **2 px edge bar** (`--pi-card-edge-width`, the toast-rail technique: `border-left` coloured by state: running `accent`, attention `statusWarning`, error `statusDanger`, otherwise transparent). Layout: header row (dot, title `font-size-sm` weight 600, daemon name as badge/meta `font-size-3xs` muted), model line (`2xs` muted), current-tool-call line (`2xs`, mono for the tool name), last-output line (`2xs` muted, one line ellipsis), footer (tokens/cost, `2xs`, "—" when unknown, D16). **Hover lift**: background to `--pi-color-surface2`, border to `--pi-color-borderAccent`, shadow `--pi-shadow-sm`; selected: border `color-mix(in srgb, var(--pi-color-accent) 45%, transparent)` + background `color-mix(in srgb, var(--pi-color-accent) 10%, var(--pi-color-surface1))`; failed turn: background `color-mix(in srgb, var(--pi-color-destructive) 10%, var(--pi-color-surface1))`; stale daemon (D25): whole card `--pi-opacity-50` with "stale since …" meta line in `statusWarning`; closed: `--pi-opacity-50`. Virtualised above ~100 cards (D28) with fixed card height so the row maths is exact |
| **Filter chip** | `surface2` fill, `1px border`, `radius-full`, `font-size-2xs`, padding `--pi-spacing-4 --pi-spacing-10`; hover `surface3`; active = tinted accent (18 % / 45 %) with `accentBright` text; status chips show a leading status dot; count suffix `foregroundMuted` |
| **Attention rail** and **rail item** | Rail: width `--pi-rail-width`, `surfaceSidebar`, left hairline `--pi-color-border`, collapsible (D15; replaced by the drill-down panel while open). Item: padding `--pi-spacing-8 --pi-spacing-10`, hover `surfaceSidebarHover`, 2 px left edge bar (`statusWarning` for a pending dialog, `statusDanger` for error/failed turn). The only item kind is the dialog attention item (next row, D36/D37) |
| **Dialog attention item** (D36) | Rail item variant for a pending `agent_ui_request` with `expectsResponse`; same edge bar (`statusWarning`), padding and hover as the rail item. Header: agent title + daemon meta (`2xs` muted), dialog `title` in `xs` semibold, optional message `2xs` muted (max 3 lines, ellipsis). Body by `method`: **confirm** — two buttons "Yes" (primary: `accent`/`accentForeground`) and "No" (secondary: `surface2` + `border`), side by side, equal width; **select** — one full-width secondary button per option (`surface2`, hover `surface3`, `font-size-xs`, gap `--pi-spacing-4`), options list capped at 8 with "+N more — open in web UI"; **input / editor** (not answerable) — a single "Open in web UI" button, outlined (`1px border`, transparent fill) with an external-link icon, plus a muted `2xs` note "needs typed input". After a click all buttons disable at `--pi-opacity-50` while in flight (D27); a resolved-elsewhere item collapses to a `foregroundMuted` line "answered elsewhere: <decision>" before removal. A pending dialog on an interrupted agent stays visible (D36). |
| **Drill-down rows** | Panel on `surfaceSidebar`; rows adapt pi-studio tool-card recipe: card body `surface1`, `1px` border, `radius-lg`; border colour by state — running `color-mix(accent 45%, transparent)`, completed `color-mix(statusSuccess 45%, transparent)`, canceled `color-mix(border 80%, transparent)`, error `color-mix(destructive 45%, transparent)`; header row `padding 7px 12px` (`--pi-spacing-7 --pi-spacing-12`), `font-size-2xs`; assistant text rows plain `foreground` `xs`; reasoning rows `foregroundMuted` italic; hover on a running card intensifies the accent wash 10 %→16 %. Bounded to 2,000 items (D17) and virtualised |
| **Status bar segments** | Height `--pi-statusbar-height`, `surface1`, top hairline `--pi-color-border`, padding `0 --pi-spacing-12`, gap `--pi-spacing-2`, `font-size-xs`; segment: `surface2` background, `radius-base`, padding `--pi-spacing-2 --pi-spacing-8`, icon `foregroundMuted`; text `max-width: 320px` ellipsis. Segments: daemons connected "N of M", agents by status, usage "N of M agents" totals (D16), view-update/diagnostic hints, last command error |
| **Toast** | pi-studio toast recipe: fixed top-centre (`--pi-spacing-16` from top), z-index 200, `surface1`, `1px border`, `border-left: 2px solid <kind rail>` (info `accent`, warning `statusWarning`, error `statusDanger`), `radius-base`, `--pi-shadow-md`, `font-size-2xs`, max-width 420 px, enter/exit 180 ms opacity+translate (`@starting-style`), no transition under reduced motion. Used for "answered elsewhere: <decision>" (D27), notices (drill-down closed: agent archived) |
| **Dialog** | Modal on a scrim; panel `surface1`, `1px border`, `radius-lg`, `--pi-shadow-lg`, padding `--pi-spacing-16`, title `font-size-md` semibold; footer buttons right-aligned. Dialogs: daemon form (name, host, port, ws/wss switch, web UI URL, optional password; inline validation: host non-empty, port 1–65535, D10), password prompt, interrupt confirm (destructive button `destructive`/`destructiveForeground`), remove daemon. Focus trapped, Esc closes (command via `ui.set`), initial focus on first field/safe button |
| **Connection pill** | Badge recipe + status dot: connecting/reconnecting `accent` (spinner), connected `statusSuccess`, auth failed / 403 `statusDanger`, unreachable `statusWarning`; "unencrypted" badge `statusWarning` for plain `ws://` + password on a non-loopback host (D32) |
| **Banner** | Full-width strip under the filter row per daemon with a degraded/auth issue: `color-mix(<token> 12%, var(--pi-color-surface1))` background, `2px` left edge bar in `<token>`, `font-size-xs`, optional action button (e.g. "Enter password", D25/D26). Tokens: warning for degraded mode/duplicate entry, danger for auth failed/403 |
| **Buttons / switch / text input** | Minimal primitives from the same tokens: button `surface2` fill + `border`, hover `surface3`, primary `accent`/`accentForeground`, destructive `destructive`; input `surface2`, `1px border`, focus ring `2px accent`; switch track `surface3`, checked `accent` |

Focus: `:focus-visible` shows a `2px` `--pi-color-accent` outline offset 1px on every interactive element. Reduced motion: `@media (prefers-reduced-motion: reduce)` removes the spinner rotation (static ring), toast and hover transitions.

### Icons

Optional: `lucide-react` (licence ISC) for the few glyphs (tray-less chevrons, close, external-link, alert). If adopted, add the npm dependency and an `ATTRIBUTION.md` row; otherwise inline SVG paths drawn for this app (no licence row needed). Decision: **inline SVG for v1** (fewer than ten glyphs, no dependency); revisit if the set grows. Radix is not used.

### Provenance (ATTRIBUTION.md)

One row is added in the same change that introduces `tokens.css` (rule 9, D20):

| Path | Title | Author | Source | Licence |
|---|---|---|---|---|
| `apps/pistudiodashboard/src/tokens.css` | Pi Studio design tokens (dark and light colour, spacing, font-size ladder, radii, shadows) | avatsaev (sole copyright holder of pi-studio) | pi-studio `packages/web-client/src/theme/{colors,variants,tokens}.ts` — values only; no source files copied | `MIT OR Apache-2.0` — the engineer relicenses the ported design values to this project (D20) |

If `lucide-react` is later added: row `lucide-react` / Lucide Contributors / lucide.dev / `ISC`.

## Behavior & algorithms

### Token integrity check (frontend test)

`tokens.test.ts` is rewritten:

```
tokens_declared   = names matched by /--pi-[\w-]+\s*:/ in tokens.css (all blocks)
for each .css/.tsx under src (excluding tokens.css, *.test.tsx):
    strip comments
    refs = every `var(--pi-…)` name
    assert each ref in tokens_declared            # CSS may reference only declared tokens
    assert no colour literal (#hex, rgb(a), hsl(a), oklch, oklab, lab, lch, color()) -- kept from scaffold
    assert `color-mix(in srgb, var(--pi-…) …)` uses only var() operands, never literals
assert dark_block names == light_block names      # parity, replaces "exactly two blocks"
assert invariant block (`:root` scales) names disjoint from the theme blocks' colour names
assert no key yields an illegal custom-property ident (no '.' in spacing keys)
assert dark values for tokens listed in the table above equal the table (value pin)
```

The value pin (or a fixtures file listing the ported values with the source pi-studio commit) is what makes acceptance criterion 14 ("values match pi-studio's") testable; pi-studio's own `token-integrity.test.ts` is the model for the "no typo'd token silently falls back" failure it guards against.

### Theme application

```
on view event:
    choice = view.ui.theme
    resolved = choice if choice in {dark, light} else (matchMedia dark ? dark : light)
    document.documentElement.dataset.theme = resolved
    if choice == system: ensure one matchMedia 'change' listener that re-resolves (remove otherwise)
```

### Snapshot verification

`npm run snapshot` (`apps/pistudiodashboard/scripts/snapshot.mjs`, Playwright; real shell view + built frontend) is run per theme with the `ui.set` script `{"theme":"dark"}` and `{"theme":"light"}` against a fixture state containing: one connected daemon with agents in every status (incl. a running card with a tool call, a needs-attention card, an error card, a closed dimmed card), one stale daemon, the attention rail with a confirm dialog and a select dialog, a toast, a banner, and the status bar. The engineer looks at both images before calling a UI change done (rule 5). `system` is verified by a unit test of the resolution function with a mocked `matchMedia`, not by snapshot. Playwright's Chromium is not the platform webview (WebKitGTK/WKWebView); platform quirks are checked in the running app.

## Data & persistence touchpoints

- The only persisted design-related datum is the theme choice (config settings, [config-and-secrets.md](config-and-secrets.md)).
- No design state in `MonitorState`.

## Error handling & edge cases

| Condition | Expected behavior |
|---|---|
| Unknown agent status (newer daemon, D26) | Outlined muted dot, "unknown" label; never a missing dot |
| `color-mix` unsupported by an older WebKit | TODO(verify) minimum WebKitGTK/WKWebView; fallback is the solid token colour with the tint omitted |
| High-contrast / forced-colors | Borders and focus outline use real colours (not shadows only) so state remains visible |
| Very long agent title, tool name or daemon name | Single-line ellipsis with full text in `title` attribute |
| OS theme changes while `system` is selected | Re-resolved live via the media-query listener |
| Token typo in a stylesheet | `tokens.test.ts` fails (no silent fallback) |

## Dependencies on other specs

- [app-structure.md](app-structure.md) — `UiState.theme`, `view.setTheme`, snapshot/control flow.
- [agent-monitor.md](../features/agent-monitor.md), [attention-and-interventions.md](../features/attention-and-interventions.md), [daemon-connections.md](../features/daemon-connections.md), [daemon-health-and-logs.md](../features/daemon-health-and-logs.md) — the content the components render.

## Acceptance criteria

- [ ] Given `tokens.css`, when the token test runs, then every `var(--pi-…)` used anywhere in `src` is declared, no colour literal exists outside `tokens.css`, and the dark and light blocks declare the same token names. (D20, D21)
- [ ] Given the value pin, then each dark and light colour/scale token equals the value in the tables above (ported from pi-studio at the recorded commit). (D20; brief acceptance 14)
- [ ] Given `view.setTheme` with `dark`, `light` and `system`, when the view arrives, then `<html data-theme>` is `dark`/`light`/the media-query result respectively, and with `system` an OS appearance change re-applies without a command. (D21)
- [ ] Given a fixture state, when `npm run snapshot` runs once per theme, then both images render the card grid, rail, status bar, toast, banner and dialog from tokens with legible contrast in both themes. (D20, D21; brief acceptance 14)
- [ ] Given an agent in each status, then the dot colour/shape matches the status table, and an agent with a pending dialog shows the warning edge bar and dot regardless of base status. (D15, D26)
- [ ] Given a closed agent or a stale daemon, then its cards render at `--pi-opacity-50`; a stale card also shows "stale since <time>". (D15, D25)
- [ ] Given an unknown status value, then the card renders an "unknown" outlined dot, never throws. (D26)
- [ ] Given `prefers-reduced-motion: reduce`, then the running spinner does not rotate and toasts do not animate. (accessibility)
- [ ] Given `ATTRIBUTION.md`, then it contains the D20 provenance row for `tokens.css` stating `MIT OR Apache-2.0`. (D20, rule 9)
- [ ] Given a card grid of 500 agents, then it virtualises (rendered DOM cards bounded by viewport) and scrolls without layout shift. (D28)

## TODO(verify)

- `accentForeground` concrete values: pi-studio derives it with `contrastForeground(accent)` in `src/theme/color-utils.ts`; read that function and pin the resulting hex for `#2e5cb8` and `#253e6f` (white is expected; do not guess).
- Light-theme `surfaceWorkspace`/`surfaceSidebar` semantics were read from `colors.ts`; confirm the dashboard's rail should use `surfaceSidebar` rather than `surface1` once a snapshot exists.
- Minimum webview versions for `color-mix(in srgb, …)` and `@starting-style` on supported Linux (WebKitGTK) and macOS (WKWebView) versions.
- Whether the card height is fixed (needed for exact virtualisation) or measured; decide with the agent-monitor spec.
- Record the pi-studio commit hash at which values were ported (for the value pin and the ATTRIBUTION row).
- Brief D20: confirm the licence statement wording for the engineer (`MIT OR Apache-2.0`) matches the repo's `LICENSE-MIT`/`LICENSE-APACHE` dual licence — evidence: both files exist at the repo root.
