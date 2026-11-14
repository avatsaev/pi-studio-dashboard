# Task 001 — Notification policy executor

- **Sprint:** sprints/sprint-008-notifications-tray
- **Status:** backlog
- **Type:** feature
- **Area:** L5 engine
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** sprint-007-attention-interventions/task-001

## Goal
Execute `Notify`/`ResolveNotification` effects with per-kind toggles, coalescing, content rules and the first-connect baseline.

## Context / why
Reducers already emit `Notify{kind, serverId, itemId}` (sprint-006/007). This task is the owner-side policy between those effects and the OS hook: what is allowed to notify, how bursts fold, and exactly which strings may leave the window (D9, D27, D32, D36).

## Scope references
- `swe/specs/features/notifications-and-tray.md` § Notification kinds and triggers, § Content rules, § Coalescing, § Resolved notifications, § Behavior & algorithms
- `swe/specs/architecture/live-state.md` § Per-event reducers (effects), § Effects
- `swe/specs/architecture/config-and-secrets.md` § Settings (notifications.needsInput/failures)
- `swe/specs/architecture/app-structure.md` § Effects, § Services hooks (`notify`)
- `crates/engine/src/` (new `notify_policy.rs`; hook into the effect executor from sprint-002/task-002; `settings_cmds.rs` or equivalent for `settings.setNotification`)
- `crates/testkit/` (recording `notify` hook, fake clock)
- `crates/config/` (`settings.notifications`)

## What to build
- Command `settings.setNotification {kind: "needsInput"|"failures", enabled: bool}`: updates the setting (default both `true`), persists through the `config_store` hook (I/O worker), updates the view's settings mirror. Turning a toggle off does not withdraw shown notifications.
- Effect execution: `Notify` → drop if the kind's toggle is off → coalescer → build `Notification{title, body, target:(serverId,itemId)}` from the item named by `itemId` at execution time → `EngineServices::notify` → remember handle (≤ 200, LRU). Hook absent → no-op with a one-time in-app notice.
- Content (single place, unit-tested): needsInput = title agent title (fallback agent id, ≤ 60) and body `<daemon name> — <method>` plus ` — <dialog title ≤ 80>` when present; failures = title agent title, body `<daemon> — turn failed` / `agent error`; summary = `Pi Studio Dashboard` / `N more items need attention` or `N items need your input`. Daemon name ≤ 40; control characters stripped. Never the dialog `message`, options, prefill, editor text, `action`, error text or any other daemon string.
- Coalescing: rolling 5 s window across all daemons/kinds; first 3 shown immediately, later ones folded into one pending summary flushed at window end ("N more items need attention", counts only; click later focuses nothing and expands the rail).
- Baseline: dialogs delivered by the first-ever successful sync (`Snapshot.uiPending`) do not notify individually; after the first sync, if the needsInput toggle is on and ≥ 1 exists, show one summary "N items need your input". After reconnects only dialogs new relative to the pre-disconnect set notify (no duplicates for re-delivered ids).
- Failure dedupe: one notification per agent per failure episode (first of `turn_failed`/`error`/status `error` until next `turn_started`); `turn_canceled` never notifies.
- `ResolveNotification`: a queued (not yet shown) notification is dropped; a shown one is withdrawn/updated through the hook where supported, else marked resolved so a click toasts "already resolved" (T002).
- Suppress notifications while the window is visible and focused (proposed, TODO(verify)); the window state comes from an `EngineServices` query/hook so the engine stays UI-free.

## Out of scope
- The real OS notification service and click callback — sprint-008/task-002.
- Tray and close-to-tray — sprint-008/task-003.
- `UiState.focusItem` rail behavior and View-menu toggles — sprint-008/task-004.
- Reducer-side effect emission — sprint-006/task-002 and sprint-007/task-001.

## Acceptance criteria
- [ ] Given `needsInput` enabled and a new confirm dialog, then the recording hook receives title = agent title and body `<daemon> — confirm — <title ≤ 80>` and nothing else.
- [ ] Given a dialog whose `message`/options hold a secret string, then that string appears in no notification field.
- [ ] Given `agent_ui_request` with `expectsResponse:false` or `removed:true`, then no notification (and no `Notify` effect arrives).
- [ ] Given `settings.setNotification {needsInput,false}`, then a new dialog produces no notification but remains an item, and the setting is persisted via the config hook.
- [ ] Given `failures` enabled, then `turn_failed`, `error` and status `error` within one episode yield exactly one notification, and `turn_canceled` yields none.
- [ ] Given 6 `Notify` effects within 5 s, then 3 individual notifications are shown and one summary "3 more items need attention".
- [ ] Given a dialog resolved before its notification is shown, then it is dropped; given it is shown, then the hook is asked to withdraw it or the handle is marked resolved.
- [ ] Given a reconnect that re-delivers the same pending dialogs, then no notification is shown; given dialogs pending at the first sync, then one summary "N items need your input" is shown.
- [ ] Given a hostile 10 KB agent/dialog title with control characters, then the notification title ≤ 60 and body title ≤ 80, with control characters stripped.

## Test / verification plan
- Tests: engine tests with the recording hook and fake clock covering toggles, content rules (including secret-leak check), coalescing window edges, baseline/reconnect, failure episodes. `cargo test -p pistudiodashboard-engine`.
- `panic_hunt`/adversarial keys for `settings.setNotification`.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- TODO(verify): dialog `title` in the notification body vs method-only (D32 judgement); suppress while window focused; baseline summary at first connect (all proposed in the spec).
- Baseline is [INFERENCE] in the spec; implement as written and keep it one function so it can be flipped.
