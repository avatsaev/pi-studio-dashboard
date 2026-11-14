# Task 002 — Primitive components

- **Sprint:** sprints/sprint-003-design-system
- **Status:** done
- **Type:** feature
- **Area:** frontend `apps/pistudiodashboard/src/components`
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-001

## Goal
Rebuild the small presentational primitives from pi-studio's recipes against the `--pi-*` tokens — StatusDot, StatusBadge, FilterChip, ConnectionPill, Banner, ToastViewport, Dialog shell, EmptyState (plus the minimal Button the dialog footer needs) — with component tests, and mount them where the skeleton already has a place (daemon chips, empty grid, toast viewport).

## Context / why
Every later UI task (daemon chips and dialogs, card grid, rail, drill-down, panels) composes these. Building them once, from tokens only, with a single status→dot translation point (D15, D26) prevents each feature inventing its own colours. They hold no state beyond what a widget needs while on screen (rule 5) and never touch daemon data.

## Scope references
- `swe/specs/architecture/design-system.md` § Components to rebuild (status mapping table, StatusDot, StatusBadge, Filter chip, Toast, Dialog, Connection pill, Banner, Buttons/switch/text input), § Focus and reduced motion, § Error handling & edge cases (unknown status, very long names), § Acceptance criteria (status dot, unknown status, reduced motion)
- `swe/specs/features/agent-monitor.md` § Status dot mapping (precedence: pending dialog > errored > base status), § View shape (`Card.dot`, `DaemonChip.conn`)
- `swe/specs/features/daemon-connections.md` § Connection states shown on daemon chips (labels/colours for the pill; read before writing the pill)
- `swe/specs/architecture/app-structure.md` § UiState (`toasts`, `dialog`), § Webview boundary
- Real files: `apps/pistudiodashboard/src/lib/types.ts`, `src/components/` (from sprint-002/task-004), `src/test-support.ts`, `src/tokens.css`
- Create: `src/components/{StatusDot,StatusBadge,FilterChip,ConnectionPill,Banner,ToastViewport,Dialog,EmptyState,Button}.tsx` + CSS Modules + tests

## What to build
All components are props-in/markup-out, tokens only (no colour literals; `tokens.test.ts` enforces), CSS Modules beside each file. Long text: single-line ellipsis with the full text in `title`.
- **`StatusDot`** — props `{ status: AgentStatus | "unknown"; attention?: "dialog" | "error" | null; label?: string }`; one pure mapping function `dotStyle(status, attention)` exported and unit-tested. Table (design-system, authoritative): `running` → spinning ring (`border-top/right` `accentBright`; `@keyframes` rotation, static ring under `prefers-reduced-motion: reduce`); `initializing` → flat `accent`; `idle` → flat `foregroundMuted`; `error` → `statusDanger`; `closed` → `foregroundMuted`; unknown value → **outlined** dot `foregroundMuted`, label "unknown"; attention overrides base colour: pending dialog → `statusWarning` (optional pulse ring, disabled under reduced motion), errored → `statusDanger`; precedence dialog > errored > base. 8px (`--pi-status-dot-size`), `role="img"` with `aria-label` containing the status word (never colour-only).
- **`StatusBadge`** — tinted pill per recipe (`padding spacing-1/spacing-6`, `radius-full`, 1px border, `font-size-xs`, weight 500; background/border via `color-mix(in srgb, <token> 18%/45%, transparent)`, text `<token>`), `tone ∈ success|danger|warning|info|accent|muted`.
- **`FilterChip`** — `surface2` fill, 1px border, `radius-full`, `2xs`, hover `surface3`; `active` = accent tint with `accentBright` text; optional leading `StatusDot`; count suffix muted; `aria-pressed`; `onToggle` callback (dispatching a command is the parent's job).
- **`ConnectionPill`** — badge + dot, input is the model `ConnState` (+ optional `lastError.kind`, `unencrypted` flag): connecting/probing/handshaking/syncing/backoff → `accent` with spinner (labels per daemon-connections spec), live → `statusSuccess`, auth_failed/host_rejected → `statusDanger`, unreachable/backoff-with-error → `statusWarning`, disabled/idle → muted; an adjacent `unencrypted` badge `statusWarning` when asked. Pure mapping function exported and unit-tested for every `ConnState` value (an unknown value renders muted, never throws).
- **`Banner`** — full-width strip: `color-mix(<token> 12%, var(--pi-color-surface1))` background, 2px left edge bar in `<token>`, `font-size-xs`, optional action button; `tone ∈ warning|danger|info`; `role="status"` (or `alert` for danger).
- **`ToastViewport`** — reads `ui.toasts` (prop), fixed top-centre (`spacing-16` from top, z-index 200), `surface1`, 1px border, 2px kind rail (`info`→`accent`, `warning`→`statusWarning`, `error`→`statusDanger`), `shadow-md`, `2xs`, max-width 420px, `@starting-style` enter/exit 180 ms opacity+translate, no transition under reduced motion. Each toast hides itself after a frontend timer (e.g. 6 s; keep errors longer) tracked in component-local state keyed by toast `id`; it never writes back to the shell. A toast re-appearing with a new `id` shows again.
- **`Dialog`** — modal shell on a scrim: panel `surface1`, 1px border, `radius-lg`, `shadow-lg`, padding `spacing-16`, title `font-size-md` semibold, footer slot with right-aligned buttons; focus trapped inside; initial focus on the first field or the safe button (prop); `Esc` and scrim click call `onClose` (the caller sends `ui.set {dialog:null}`); restores focus to the opener on close; `role="dialog"`, `aria-modal`, `aria-labelledby`. Content/forms are not built here.
- **`Button`** — minimal variants `secondary` (`surface2` + border, hover `surface3`), `primary` (`accent`/`accentForeground`), `destructive` (`destructive`/`destructiveForeground`), disabled state with `title` reason.
- **`EmptyState`** — centred icon-less block: title, optional detail line, optional action button; used by the grid area (replaces the inline empty text from sprint-002/task-004; keep the wording).
- **Wiring in the existing skeleton:** `DaemonChips` renders each chip with `ConnectionPill`; the grid area uses `EmptyState`; `App` mounts `ToastViewport` fed by `view.ui.toasts`. No new behaviour beyond drawing the view.
- Icons: **inline SVG** for the few glyphs needed (close, alert), no icon dependency (spec § Icons).
- Tests (vitest + testing-library, no snapshot-of-markup tests): `StatusDot` mapping table incl. unknown, precedence and `aria-label`; reduced-motion renders the static ring (mock `matchMedia`/class check); `ConnectionPill` mapping for every `ConnState` incl. unknown value; `FilterChip` toggle; `Dialog` Esc/scrim → `onClose`, focus trapped/initial focus/restored; `ToastViewport` shows `ui.toasts` and hides after the timer (fake timers); `Banner` tones/actions; `EmptyState` action.

## Out of scope
- Agent card, rail items, drill-down rows (sprints 006/007), the daemon form/password/remove/interrupt dialogs' content (sprints 005/007), diagnostics/health/logs panels (sprints 005/010).
- Filter chip behaviour/commands, virtualisation (sprint-006/task-004).
- `ATTRIBUTION.md` and snapshots → task-003.

## Acceptance criteria
- [ ] Given an agent in each status, then the dot colour/shape matches the status table; given an agent with a pending dialog (any base status), then the dot is `statusWarning`; given errored (no dialog), `statusDanger`. (design-system ACs, D15, D26)
- [ ] Given an unknown status string, then `StatusDot` renders an outlined muted dot with label "unknown" and never throws.
- [ ] Given `prefers-reduced-motion: reduce`, then the running dot does not rotate and toasts do not animate.
- [ ] Given every `ConnState` value (and one unknown value), then `ConnectionPill` renders the documented tone and label; `unencrypted` shows its warning badge only when requested.
- [ ] Given `view.ui.toasts` with a new toast, then it is shown and disappears after the timer without any backend call; a long text is ellipsised with the full text in `title`.
- [ ] Given an open `Dialog`, then Tab cycles inside it, Esc and scrim click call `onClose`, and focus returns to the previously focused element after closing.
- [ ] Given the sample view, then daemon chips show `ConnectionPill`s and an empty monitor shows the `EmptyState`.
- [ ] `tokens.test.ts` passes (all new CSS uses declared `--pi-*` tokens, no colour literals).

## Test / verification plan
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test`.
- UI: `npm run snapshot` with a saved view that includes a toast, one live and one backoff daemon chip, in dark and light — look at the images (formal recording is task-003; fix defects now).
- Gate: `cargo xtask ci` passes.

## Notes
- **Resolved (plan review):** `closed` uses a muted dot, never green (design-system.md wins; agent-monitor.md was aligned). The view carries `status` + `attention` and no `dot` field (sprint-006/task-004 updated); `StatusDot` takes `status` + `attention`.
- TODO(verify) (design-system.md): minimum WebKitGTK/WKWebView for `color-mix(in srgb, …)` and `@starting-style`; fallback is the solid token colour — implement the fallback only if a real platform check (sprint-009) shows breakage.
- Card height fixed vs measured (virtualisation) is decided in sprint-006/task-004.
- Focus: `:focus-visible` outlines already come from task-001; do not hide outlines on any new element.
