# Task 003 — Tray icon and close-to-tray

- **Sprint:** sprints/sprint-008-notifications-tray
- **Status:** backlog
- **Type:** feature
- **Area:** app binary + L5 engine
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Add a tray icon with show/hide, quit and attention counts, and a persisted close-to-tray setting that keeps the process alive.

## Context / why
Notifications are only useful when the process survives window close (D14, D34). Tray code lives in the app binary behind a hook; the engine only owns the persisted setting and its disabled reason.

## Scope references
- `swe/specs/features/notifications-and-tray.md` § Tray, § Close-to-tray, § Platform notes
- `swe/specs/architecture/config-and-secrets.md` § Settings (closeToTray)
- `swe/specs/architecture/app-structure.md` § Services hooks, § Threads
- `apps/pistudiodashboard/src-tauri/src/` (new `tray.rs`; wiring in `main.rs`, `services.rs`)
- `crates/engine/src/` (`settings.setCloseToTray`)
- `crates/config/` (`settings.closeToTray`)
- `apps/pistudiodashboard/src-tauri/Cargo.toml` (tray feature)

## What to build
- Tray (Rust only): menu "Show / Hide Pi Studio Dashboard", disabled info line "N need attention · M running" from the view totals (refreshed on view changes, coalesced), "Quit" (exits even with close-to-tray on). Tooltip/badge with the attention count; attention icon variant only if cheap, otherwise static icon + menu text (spec accepts it). Tray menu actions post owner-thread requests; the tray holds no state.
- `tray.show_window` hook (show + unminimize + focus) replaces the fallback used by `window.focusItem` in T002.
- Command `settings.setCloseToTray {enabled}`: persisted via `config_store`; enabled only when a tray hook reports availability, else disabled with "no tray available" and a notice. Default: on when a tray is available (TODO(verify)).
- Window close request: if `settings.closeToTray` and tray available → prevent close, hide window, keep process/connections/notifications alive; else exit normally. No tray host (e.g. Linux without StatusNotifier) → close-to-tray disabled and closing quits.
- Check the licence of any additional tray crate/feature (golden rule 9), record in the summary.

## Out of scope
- Notification content/click — sprint-008/task-001/002.
- View-menu checkbox for close-to-tray and focus rendering — sprint-008/task-004.
- Windows tray (deferred).

## Acceptance criteria
- [ ] Given close-to-tray enabled and a tray available, when the window is closed, then the window hides while the process, daemon connections and notifications continue.
- [ ] Given close-to-tray disabled (or no tray), when the window is closed, then the app quits.
- [ ] Given the tray menu, then Show/Hide toggles the window, the info line shows "N need attention · M running" matching the view totals, and Quit exits even with close-to-tray on.
- [ ] Given no tray host available, then `settings.setCloseToTray {enabled:true}` is refused with "no tray available" and a notice is shown.
- [ ] Given `settings.setCloseToTray`, then the value persists across restart.
- [ ] Given a tray action, then it reaches the session only as an owner-thread request.

## Test / verification plan
- Tests: engine tests for `settings.setCloseToTray` (hook present/absent, persistence call); unit tests for the info-line formatter and the close-request decision function (pure).
- Manual (Linux with an appindicator host and macOS if available): close/restore from tray, quit, counts.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- TODO(verify): default of close-to-tray (proposed on); Linux tray requires a StatusNotifier/appindicator host.
- Capture platform observations in the summary.
