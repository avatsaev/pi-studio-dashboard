# Feature — Notifications & Tray

> Part of: [overview.md](../overview.md)
> Dependencies: [attention-and-interventions.md](attention-and-interventions.md), [live-state.md](../architecture/live-state.md), [app-structure.md](../architecture/app-structure.md), [config-and-secrets.md](../architecture/config-and-secrets.md)
> Source brief: [daemon-monitoring-dashboard.md](../../grills/daemon-monitoring-dashboard.md) (D9, D14, D27, D32, D34, **D36**, **D37**; happy path 6, 9; acceptance 8)

## Purpose
Keep the engineer informed while the window is closed or in the background: OS notifications when an agent needs input or fails, a tray icon that keeps the process alive, and click-through that lands on the exact item. All OS integration lives in the app binary behind `EngineServices` / tray hooks; the webview is never granted notification, tray or window permissions (golden rule 6).

## Public contract

### Commands (persisted via config spec)
| Command id | Args | Effect |
|------------|------|--------|
| `settings.setNotification` | `{ kind: "needsInput" \| "failures", enabled: bool }` | Updates the toggle (persisted, D9; default both `true`) |
| `settings.setCloseToTray` | `{ enabled: bool }` | Persisted (D14); enabled only if the tray hook is available |
| `window.focusItem` | `{ serverId, itemId }` (internal, no menu; app-structure) | Shows/raises the window and sets `UiState.focusItem` |

### Effects → hooks (app-structure Effects table)
`Notify{kind, serverId, itemId}` → `EngineServices::notify(Notification{title, body, target: (serverId, itemId)})` after toggle and coalescing are applied. `ResolveNotification{serverId, itemId}` marks/withdraws a shown notification. The notifier builds `title`/`body` from the item named by `itemId` at execution time (so content rules live in one place).

### Notification kinds and triggers (D9, D36)
| Kind (toggle) | Trigger | Not a trigger |
|---------------|---------|---------------|
| `needsInput` | A **new** `UiRequestItem` with `expectsResponse == true` (any `agent_ui_request` dialog: confirm, select, input, editor, other; hand-off ones included, since the engineer must act somewhere) inserted by a live `UiRequest`. Dialogs are the only `needsInput` source (D37) | `expectsResponse == false` surfaces (`notify`, `setStatus`); `removed`; items present in a connect `Snapshot` (baseline rule below); re-delivery of an existing key |
| `failures` | `AgentStream.turn_failed`; `AgentStream.error`; agent status transition to `error` | `turn_canceled`; further failure signals in the same episode |

Failure episode (dedupe): from the first of {`turn_failed`, `error`, status `error`} until the agent's next `turn_started`: one notification per agent per episode.

Baseline rule: dialogs already pending when a daemon connects (`Snapshot.uiPending`) do not fire individual notifications. After the **first** successful sync of the app session, if the toggle is on and at least one such dialog exists, one summary notification "N items need your input" is shown (counts only). After reconnects, only dialogs that are new relative to the pre-disconnect set notify. [INFERENCE: avoids both silence for a dialog pending at launch and storms on every reconnect.]

### Content rules (D32, D36)
| Kind | Title | Body |
|------|-------|------|
| `needsInput` dialog | agent title (fallback agent id), ≤ 60 chars | `<daemon name> — <method>` plus, when the dialog has a `title`, ` — <title truncated to 80 chars>`; e.g. `local — confirm — Delete 3 files?` |
| `failures` | agent title | `<daemon name> — turn failed` / `agent error` |
| summary (coalescing/baseline) | `Pi Studio Dashboard` | `N more items need attention` / `N items need your input` |
Never included: dialog `message`, options, prefill/placeholder, editor text, `action`, command/path/error text, any other daemon string. All strings are length-capped (daemon name 40, tool 40) with control characters stripped (D28, D32). Decision: the dialog `title` **is** allowed (≤ 80 chars) because it names the question the engineer must answer; it is the only payload text that may leave the app window. [TODO(verify): engineer may prefer no title on lock screens; if so, method only.]

### Coalescing (D27)
```
window = rolling 5 s over the app (all daemons, both kinds)
on Notify: if shown_in_window < 3: show now; else: add to a pending summary and schedule one flush at window end
flush: ONE notification "N more items need attention" (counts only); click → window shown, rail expanded, no highlight
```
(">3 in 5 s → one summary": notifications 4+ in the window are folded into the summary.)

### Resolved notifications (D27)
When an item is resolved (answered here or elsewhere, timed out, cancelled) while its notification is shown or queued: a not-yet-shown notification is dropped; a shown one is withdrawn/updated where the platform allows; otherwise a click shows the window and the toast "already resolved". The toggle being turned off does not withdraw already shown notifications.

### Click → focus the item (D14)
```
OS click (callback thread) → app binary maps handle → (serverId, itemId)
   → posts Request::Command("window.focusItem", {serverId, itemId}) on the owner-thread channel     # callback never touches Session
owner thread: window.focusItem:
    tray.show_window (show + unminimize + focus)
    if item exists: UiState.focusItem = {serverId, itemId}; rail expands (a drill-down in progress is closed so the rail is visible);
                    frontend scrolls/highlights; highlight clears after 3 s or on next interaction
    else: Toast("already resolved")
summary click: show window, rail expanded, no focusItem
```
Click focuses the **attention-rail item** (its answer buttons live there), not the drill-down; failure items offer "View agent" (`agent.select`).

### Tray (D14, D34)
- Tray icon created by the app binary (Tauri tray support, Rust-side only).
- Menu: "Show / Hide Pi Studio Dashboard", a disabled info line "N need attention · M running" from the view totals, "Quit" (exits even with close-to-tray on).
- Optional attention icon variant when attention items > 0 [INFERENCE; static icon + menu text is acceptable if unsupported].
- The tray holds no state; menu actions dispatch owner-thread requests.

### Close-to-tray (D14)
```
on window close request: if settings.closeToTray and tray available: prevent close; hide window; process, connections and notifications continue
                         else: exit
```
No tray host → close-to-tray is disabled with a notice and closing quits.

### Platform notes (D34)
- Linux: freedesktop notifications; tray needs a StatusNotifier/appindicator host; click callbacks/actions depend on the notification server — where unsupported the tray is the entry point.
- macOS: first run needs the user to allow notifications in System Settings (unsigned build, manual first run); close-to-tray = menu-bar icon with the app running.
- Windows deferred. Permissions are requested and used by Rust only; the webview capability grants none (golden rule 6). Licences per golden rule 9.

## Behavior & algorithms
```
reducers (live-state) emit Notify/ResolveNotification effects (deduped by itemId; pure, tested)
owner thread executes: if !settings.notifications[kind]: drop
                       apply coalescer → build title/body from the item → EngineServices::notify → remember handle (≤ 200, LRU)
click → owner-thread command (above)
```
Notifications are shown only when the window is hidden or unfocused [INFERENCE; TODO(verify)] since the rail already shows the item when the window is focused.

## Data & persistence touchpoints
- Persisted: toggles (`needsInput`, `failures`), `closeToTray` (config `settings`).
- In memory (app binary): coalescer window, handle map. No notification history (D8).

## Error handling & edge cases
| Condition | Expected behavior |
|-----------|-------------------|
| OS notification service unavailable / denied | Log (no content); one-time in-app notice; tray menu counts still work |
| Burst > 3 in 5 s | One summary (D27) |
| Dialog resolved before its notification is shown | Dropped |
| Click for removed daemon/agent/item | Window shown; toast "already resolved" |
| Callback off-thread | Forwarded via the owner-thread channel only |
| Window visible and focused | Suppressed (see TODO) |
| No tray and close-to-tray on | Close quits |
| Both toggles off | No OS notifications; rail and tray unaffected |
| Same request delivered twice | One notification (idempotent itemId) |
| Hostile/long agent title or dialog title | Capped, control characters stripped |

## Dependencies on other specs
- [live-state.md](../architecture/live-state.md) — effects, dedupe, baseline.
- [attention-and-interventions.md](attention-and-interventions.md) — item ids and focus target.
- [app-structure.md](../architecture/app-structure.md) — hooks, `UiState.focusItem`, capabilities.
- [config-and-secrets.md](../architecture/config-and-secrets.md) — toggles persistence.

## Acceptance criteria
- [ ] Given the window closed to the tray, when a `confirm` dialog arrives, then an OS notification shows agent title and `<daemon> — confirm — <title ≤ 80>` only, and clicking it shows the window with that item highlighted in the rail. (D9, D14, D32, D36; brief 8)
- [ ] Given a dialog whose `message`/options hold secrets, then none of that text appears in the notification. (D32)
- [ ] Given `agent_ui_request` with `expectsResponse:false` (setStatus/notify), then no notification. (D36)
- [ ] Given `settings.setNotification{kind:"needsInput", enabled:false}`, then a new dialog produces no OS notification, still appears in the rail, and the setting persists across restart. (D9)
- [ ] Given `failures` enabled, when `turn_failed`, `error` or status `error` occurs, then one notification per failure episode is shown; `turn_canceled` shows none. (D9)
- [ ] Given 6 notifications in 5 s, then 3 individual ones and one summary "3 more items need attention". (D27)
- [ ] Given a dialog answered elsewhere before the notification is clicked, then the notification is withdrawn/marked and a click shows "already resolved". (D27)
- [ ] Given the daemon reconnects with the same pending dialogs, then no duplicate notifications. (D25, D27)
- [ ] Given dialogs pending at first connect, then one summary notification (not one each). (D36)
- [ ] Given close-to-tray enabled, when the window is closed, then process, connections and notifications continue; disabled → closing quits. (D14)
- [ ] Given the tray menu, then Show/Hide toggles the window, the info line shows attention/running counts, and Quit exits even with close-to-tray on. (D14)
- [ ] Given a click on a callback thread, then it reaches the session only as an owner-thread command. (golden rule 5; D14)
- [ ] Given the webview capability file, then it has no notification, tray or window-management permission. (golden rule 6)

## TODO(verify)
- Dialog `title` in notification body (proposed ≤ 80 chars) vs method-only (D32 judgement).
- Default of close-to-tray (proposed on).
- Suppress notifications while the window is focused (proposed yes).
- Baseline summary at first connect (proposed).
- Tauri tray/notification plugin capabilities per platform (Linux click callbacks, update/withdraw, tray click events) and licences (golden rule 9).
