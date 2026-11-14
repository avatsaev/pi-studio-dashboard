# Task 002 — OS notifications and click-to-focus

- **Sprint:** sprints/sprint-008-notifications-tray
- **Status:** backlog
- **Type:** feature
- **Area:** app binary (`src-tauri`)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-001

## Goal
Show real OS notifications from Rust only and route a click to the owning item via the owner thread.

## Context / why
Golden rule 6: the webview gets no notification permission. The click callback runs on an OS thread and must never touch `Session`; it only posts a command on the owner-thread channel (golden rule 5; D14).

## Scope references
- `swe/specs/features/notifications-and-tray.md` § Click → focus the item, § Platform notes, § Error handling & edge cases
- `swe/specs/architecture/app-structure.md` § Threads, § Services hooks, § Webview boundary/capabilities
- `apps/pistudiodashboard/src-tauri/src/services.rs` (real `notify` hook)
- `apps/pistudiodashboard/src-tauri/src/bridge.rs` (owner-thread request channel)
- `apps/pistudiodashboard/src-tauri/capabilities/default.json` (must gain NO notification/tray/window permission)
- `crates/engine/src/` (`window.focusItem` command)
- `apps/pistudiodashboard/src-tauri/Cargo.toml`

## What to build
- Implement the `notify` hook in the app binary using a Rust-side notification library (Tauri notification plugin used from Rust only, or a lighter crate such as `notify-rust` — decide by the TODO(verify) capability check below). Record the chosen crate's **licence** (permissive only, golden rule 9) in the task summary.
- Handle map: `(serverId,itemId)` ↔ platform notification handle, bounded 200 LRU, shared with the policy from T001 for withdraw/mark-resolved.
- Click callback (any thread) → `Request::Command("window.focusItem", {serverId, itemId})` posted on the owner-thread channel; summary notifications post a command that shows the window and expands the rail without a target.
- Command `window.focusItem {serverId, itemId}` (internal, no menu): runs `tray.show_window`-equivalent hook (show + unminimize + focus; falls back to the main window when the tray is not yet built — T003 later swaps in the tray hook); if the item exists sets `UiState.focusItem = {serverId,itemId}`, expands the rail and closes an open drill-down so the rail is visible; else `Toast("already resolved")`. Highlight expiry (3 s) is a frontend concern (T004).
- Service unavailable/denied → log without content, one-time in-app notice; tray/menu counts still work. Where click callbacks are unsupported (Linux notification servers) the notification still shows and the tray (T003) is the entry point.
- Capability file stays free of notification/tray/window-management permissions; add a test or an `xtask`-level check if one exists for capabilities.

## Out of scope
- Policy/toggles/coalescing — sprint-008/task-001.
- Tray icon, close-to-tray — sprint-008/task-003.
- Rail scroll/highlight rendering and settings menu — sprint-008/task-004.
- Windows support (deferred).

## Acceptance criteria
- [ ] Given the window hidden and a confirm dialog arriving, then an OS notification appears with the agent title and `<daemon> — confirm — <title>`; clicking it shows the window with `UiState.focusItem` set to that item.
- [ ] Given a click whose item was already resolved, then the window is shown and the toast reads "already resolved".
- [ ] Given a click callback on a non-owner thread, then the only effect is a message on the owner-thread channel (verified by a unit test of the callback adapter with a fake channel).
- [ ] Given the notification service is unavailable, then the app keeps running, logs without content and shows a single in-app notice.
- [ ] Given `capabilities/default.json`, then it contains no notification, tray or window-management permission.
- [ ] Given a recorded licence for the chosen notification crate, then it is permissive and noted in the summary.

## Test / verification plan
- Tests: engine tests for `window.focusItem` (item present/absent, drill closed); unit test for the callback adapter; capability-file assertion.
- Manual (Linux and, if available, macOS): trigger a dialog with the window hidden, observe the notification, click it.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- TODO(verify): Tauri/notification crate capabilities per platform (Linux click callbacks, update/withdraw) and licences.
- macOS: first run needs the user to allow notifications (unsigned build); document in the summary what was observed.
