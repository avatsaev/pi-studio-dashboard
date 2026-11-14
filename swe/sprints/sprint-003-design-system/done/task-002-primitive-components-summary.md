# Task 002 — Primitive components — Summary

- **Sprint:** sprints/sprint-003-design-system
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
Nine presentational primitives, props in / markup out, tokens only, CSS Modules beside each, inline-SVG icons (`icons.tsx`: close, alert; no icon dependency):
- `StatusDot` + exported pure `dotStyle(status, attention)` = the single status→dot translation (running → spinning ring `accentBright`; initializing → flat `accent`; idle/closed → flat `foregroundMuted`; error → `statusDanger`; unknown value → outlined muted dot labelled "unknown"; attention overrides, precedence dialog > errored > base: pending dialog → flat `statusWarning` + pulse, errored → `statusDanger`). `role="img"` + `aria-label` (status word, e.g. "idle, needs a response"). Every shape sits in one fixed slot so a row does not shift when a dot becomes a ring.
- `StatusBadge` (tinted pill, `color-mix(in srgb, <tone> 18%/45%, transparent)`), `FilterChip` (`aria-pressed`, optional leading dot, count, `onToggle`), `ConnectionPill` + exported pure `pillSpec(conn, {wasLive, errorKind})`, `Banner` (12 % tint over `surface1`, 2 px edge, `role="alert"` for danger else `status`, optional action), `ToastViewport` (reads `ui.toasts`; per-toast frontend timer 6 s / error 12 s, 180 ms exit, tracked by toast `id` in local state, new id shows again, local dismiss button, one-line ellipsis with full text in `title`), `Dialog` (portal to `<body>`, native `<dialog open>` panel on a scrim, trapped Tab/Shift+Tab, initial focus on `initialFocus` ref else first field else first focusable, `Esc` and scrim mousedown → `onClose`, focus restored to the opener, `aria-modal`, `aria-labelledby`), `Button` (`secondary`/`primary`/`destructive`, `disabledReason` → disabled + `title`), `EmptyState` (title, optional detail, optional action).
- Wiring: `DaemonChips` draws each chip with a `ConnectionPill` (name ellipsis + `title`), `CardGrid` uses `EmptyState` (same wording) and puts a `StatusDot` (status + attention from the card) in the placeholder card header, `App` mounts `ToastViewport` fed by `view.ui.toasts`.
- `app.css` default button hover now has zero specificity (`:where(…)`) so a component's own hover colour wins.
- `scripts/snapshot.mjs` waits for finite animations (a toast's enter transition) before the screenshot; endless ones (spinners) keep running. Found because the first toast snapshots were captured mid-fade.

## Files created / changed
| File | Change |
|------|--------|
| `src/components/{StatusDot,StatusBadge,FilterChip,ConnectionPill,Banner,ToastViewport,Dialog,Button,EmptyState}.tsx` + `.module.css` | created |
| `src/components/icons.tsx` | created |
| `src/components/{StatusDot,ConnectionPill,FilterChip,Banner,ToastViewport,Dialog,Button,EmptyState}.test.tsx` | created (StatusBadge is covered through ConnectionPill/Banner/App) |
| `src/components/{DaemonChips,CardGrid}.tsx`, `DaemonChips.module.css`, `CardGrid.module.css` | modified |
| `src/App.tsx`, `src/App.test.tsx`, `src/app.css`, `src/test-support.ts` (`reducedMotionCss`), `src/tokens.test.ts` (mix operands may be any `var(--…)` so a component can pass a local `--tone`; plus `transparent`/`currentColor`) | modified |
| `scripts/snapshot.mjs` | modified (wait for finite animations) |
| `swe/sprints/sprint-003-design-system/done/screenshots/s003-t002-*.png` | 13 PNGs |

## How it satisfies the scope
Follows design-system § Components to rebuild, § Focus and reduced motion, agent-monitor § Status dot mapping. Decisions and deviations:
- **Status dot size:** the dot is `--pi-status-dot-size` (8 px); the ring is 12 px (pi-studio found an 8 px ring vanished on dark) inside a 12 px slot for every shape.
- **`accent` badge text** uses `accentBright` (tint from `accent`): solid `#2e5cb8` as text is unreadable on dark surfaces; same choice as the active filter chip in the spec.
- **ConnectionPill backoff** (the task text and daemon-connections disagree on colours): backoff is `warning` ("Unreachable") when never live or when an `errorKind` is known (then "Reconnecting" if it had been live); backoff after having been live with no error known yet is `accent` + spinner "Reconnecting…". `probing/connecting/handshaking/syncing` → accent spinner "Connecting…"; `live` → success "Connected"; `auth_failed` "Auth failed" and `host_rejected` "Host not allowed" → danger; `idle` "Disconnected" and `disabled` "Disabled" → muted; unknown → muted "Unknown". `DaemonChip` has no error kind yet, so the chips pass only `wasLive = staleSince !== null`.
- **Scrim** is `color-mix(in srgb, var(--pi-color-surface0) 70%, transparent)` (no colour literal allowed; pi-studio uses `rgba(0,0,0,.6)`): near-black over dark, a white veil over light, panel kept apart by border + `shadow-lg`.
- **Reduced motion:** component CSS sets `animation: none` for the ring, pulse and connection spinner and `transition: none` for toasts; `app.css` adds a global instant-transition rule. Tests read the stylesheets' reduced-motion block (`reducedMotionCss`), and it was confirmed in a real browser (below).
- `StatusBadge` and the `Dialog` `onClose` are props; no component writes to the shell.
- `dot` field of the old `Card` type is not read (`StatusDot` takes status + attention); `types.ts` is unchanged here (Rust-owned, handled in the sprint-006 plan).

## Build & test results
```
$ cd apps/pistudiodashboard && npm run check     # tsc --noEmit: clean
$ npm run lint                                   # oxlint --deny-warnings: clean
$ npm test                                       # 14 files, 123 tests passed, 0 failed
$ npm run build                                  # ok; `@starting-style` kept in the CSS bundle
```
Real-browser reduced-motion probe (throwaway Playwright page with `reducedMotion: "reduce"` vs default, computed styles): running ring `animation-name` `_spin_…` → `none`; warning pulse `_pulse_…` → `none`; connection spinner `_spin_…` → `none`; toast `transition-duration` 0.18 s → 0.00001 s.

## Acceptance criteria
- [x] Each status draws the table's shape/colour; any status + pending dialog → warning; errored without dialog → danger (StatusDot.test, App.test)
- [x] Unknown status string → outlined muted dot labelled "unknown", never throws (StatusDot.test)
- [x] Reduced motion: running ring and pulse do not rotate, toasts do not animate (StatusDot/ConnectionPill/ToastViewport tests + real-browser probe)
- [x] Every `ConnState` value and an unknown one → documented tone and label; `unencrypted` badge only when requested (ConnectionPill.test)
- [x] New toast in `view.ui.toasts` shows, disappears after its timer without a backend call; long text ellipsised with full `title` (ToastViewport.test, App.test asserts no calls)
- [x] Open `Dialog`: Tab/Shift+Tab cycle inside, Esc and scrim click call `onClose`, focus returns to the opener (Dialog.test)
- [x] Sample view → chips with `ConnectionPill`s; empty monitor → `EmptyState` (App.test, snapshots)
- [x] `tokens.test.ts` passes: all CSS uses declared `--pi-*` tokens, no colour literals (token test)

## Snapshots (rule 5)
Live views: the real-daemon view (7 agents from the dev daemon at 127.0.0.1:6790, via the shell `feed.ingest` as in sprint-002/task-004) enriched with a toast pair and three more daemon chips (`laptop (stale)` backoff + `staleSince`, `build-box` auth_failed, `new-daemon` connecting) and `pendingDialogs: 1` on two of the three running "#ui confirm" agents (they genuinely hold a pending confirm dialog on the daemon). Primitives gallery: a throwaway Vite page (deleted afterwards) rendering every state of every primitive, plus an open `Dialog`, in both themes.
- `s003-t002-live-chips-{dark,light}.png`: chip row `workstation [● Connected] 7`, `laptop (stale) [◌ Reconnecting…] 0` (accent spinner), `build-box [● Auth failed]` (danger), `new-daemon [◌ Connecting…]`; cards: red dot `#stream fail`, two amber dots (pending dialog on running agents), one running ring, three grey idle dots; all text legible, no overflow.
- `s003-t002-live-toast-{dark,light}.png`: info toast (blue rail) and error toast (red rail, long text ellipsised to "…This is a delibe…") fixed top-centre over the header with shadow, fully opaque after the snapshot-wait fix (first capture was mid-fade; that was the script, not the CSS).
- `s003-t002-empty-{dark,light}.png`: centred `EmptyState` title in muted text.
- `s003-t002-primitives-{dark,light}.png`: dots in all 7 statuses (incl. two unknown values as outlined rings) × plain/dialog/error; six badge tones + version; chips rest/active/with dot; 13 connection pills; three banners with action; four buttons (disabled dimmed); empty state with detail and action.
- `s003-t002-primitives-toasts-light.png`, `s003-t002-dialog-{dark,light}.png` (panel on scrim, Cancel focused with a visible ring; light panel separated by border and shadow over a pale veil), `s003-t002-focus-ring-dark.png` (2 px accent ring on a keyboard-focused chip).
Defects found and fixed: toast captured mid-fade (snapshot wait); none in CSS. Observation: dark `initializing` (`accent` #2e5cb8) is the dimmest dot but still readable against `surface1`.

## Smoke against pi-studio
Live agents from the dev daemon (see above) are drawn by the real frontend through the real shell view with the new dots/chips/toasts. The app has no client wiring yet (sprint-005), so no window/connection smoke is possible; no window opened.

## Follow-ups / TODO(verify)
- `color-mix` and `@starting-style` minimum WebKitGTK/WKWebView still to be checked in the real app (sprint-009); no fallbacks added.
- The `dot` field on the TS `Card` type is unused; remove with the Rust view change (sprint-006/task-004).
- `ConnectionPill` will take `errorKind` from `DaemonChip.lastError` once the model carries it (sprint-005).
