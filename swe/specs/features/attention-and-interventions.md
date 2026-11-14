# Feature — Attention Rail & Interventions

> Part of: [overview.md](../overview.md)
> Dependencies: [live-state.md](../architecture/live-state.md), [agent-monitor.md](agent-monitor.md), [notifications-and-tray.md](notifications-and-tray.md), [protocol-client.md](../architecture/protocol-client.md), [daemon-connections.md](daemon-connections.md), [app-structure.md](../architecture/app-structure.md), pi-studio [extension-ui-rpc.md](../../../../pi-studio/swe/features/extension-ui-rpc.md), [web-ui-agent-deep-link.md](../../../../pi-studio/swe/features/web-ui-agent-deep-link.md)
> Source brief: [daemon-monitoring-dashboard.md](../../grills/daemon-monitoring-dashboard.md) (D2, D7, D9, D13, D14, D24, D27, D28, D32, **D36**, **D37**; happy path 6–9; acceptance 3–7, 12)

## Purpose
Show everything that needs the engineer in a collapsible right rail and provide the only two interventions the dashboard may perform: **answer a Pi dialog** (`confirm` → Yes/No, `select` → one button per option) and **interrupt a running agent** (D2, D36). Everything else hands off to the web UI.

Per D36 and D37 the answerable items are Pi dialogs, i.e. `agent_ui_request` with `expectsResponse: true`, answered by `agent_ui_respond_request` and caught up on connect via the existing `agent_ui_list_request` (`pending[]` only; `surfaces[]` are not attention items). There is no other answerable channel: pi-studio retired the tool-permission family (D37).

## Public contract

### Commands
| Command id | Args | Effect | Enabled when / reason |
|------------|------|--------|-----------------------|
| `dialog.respond` | `{ serverId, requestId, response }` (`requestId` = the dialog's daemon-minted id) with `response = {confirmed: bool}` (confirm) or `{value: string}` (select; `value` MUST equal one of the item's `options`) | Marks the item in flight; sends `agent_ui_respond_request {requestId(rpc), uiRequestId: requestId, response}` (fixtures `agent_ui_respond_request.confirm.json` / `.select.json`) via `EngineServices::send_to_daemon` | item exists; `answerability == Answerable`; not `inFlight`; not expired; daemon connected and not stale; response valid for the item kind. Reasons: "already sending", "expired", "daemon disconnected", "answer in web UI", "invalid option" |
| `agent.interrupt` | `{ serverId, agentId, confirm: bool }` (app-structure: `confirm` MUST be `true` to send) | Frontend shows `UiState.dialog = confirmInterrupt{serverId, agentId}`; submit calls with `confirm:true` → `interrupt_agent {agentId}` | agent `running` (or `initializing`), daemon connected, no interrupt in flight. Reasons: "not running", "daemon disconnected", "interrupt in progress" |
| `agent.openInWebUi` | `{ serverId, agentId }` | `open_url(<URL below>)` | daemon entry has a web UI URL; else disabled "no web UI URL configured for <daemon>" (D24) |

Daemon mutations are limited to the protocol-client `MUTATION_ALLOWLIST` = exactly `agent_ui_respond_request` and `interrupt_agent` (D2 as revised by D36, D37). No other command can send a daemon mutation (D2).

### Attention item (derived view of `MonitorState`; never stored separately)
```
AttentionItem = {
  itemId, kind: dialog | failure,
  serverId, daemonName, agentId, agentTitle, raisedAt, stale,
  dialog?:     { method, kind: Confirm|Select|Input|Editor|Other, title?, message?, options?, answerability, inFlight,
                 expiresAt?, expired, handOffReason? },
  failure?:    { message?, source: turnFailed | agentStatusError },
  actions: [ { id, label, enabled, reason?, command, args } ]            # computed by the engine
}
```
`itemId` is the `focusItem.itemId` of app-structure (`"ui:<requestId>"`, `"fail:<agentId>"`). The frontend draws `actions` verbatim and holds no rule logic (golden rule 5).

Rail order: newest first by `raisedAt`; stale items sink to the bottom. Header shows the count; collapsed rail shows a count badge. The rail ignores card filters (a filtered-out pending dialog must stay visible). While the drill-down is open the rail is replaced (D15); the drill-down header repeats that agent's pending dialogs inline.

## Behavior & algorithms

### What creates and clears items
| Event (`DaemonEvent` kind) | Creates | Clears |
|----------------------------|---------|--------|
| `UiRequest` with `expectsResponse == true` and not `removed` (live), or an entry of `Snapshot.uiPending` (late-join via `agent_ui_list_request`) | `dialog` item keyed `(serverId, requestId)`; idempotent upsert | `UiResolved{requestId, reason}` (daemon reasons `answered`, `timeout`, `aborted`; `cancelled` is schema-only and tolerated); agent archived/deleted; `Snapshot` omitting it; daemon entry removed; `RequestResult` of our answer with `ok:false, error:"not_found"` |
| `UiRequest` with `expectsResponse == false` (`setStatus`, `notify`, …) or `removed == true` | **nothing** — surfaces are not attention items (D36) | — |
| `AgentStream.turn_failed` | `failure` item keyed per agent | next `turn_started`; agent leaves `error` status; agent archived/deleted |
| agent status becomes `error` (`AgentUpdate`/`AgentStatus`/`Snapshot`) | `failure` item (merged with a `turnFailed` item of the same agent: one failure item per agent) | status changes away from `error`; `turn_started` |
| `AgentStream.error` (non-terminal) | no rail item (event ring + notification only; may precede a terminal event) | n/a |

**Interrupt leaves dialogs pending** (D36; pi-studio `extension-ui-rpc.md`: "Interrupt touches nothing"). The dashboard never clears dialog items on interrupt; they clear only by `UiResolved`, `Snapshot`, or removal of the agent.

Card flags (`attention.pendingDialogs`, `errored`) come from the same reducers, so card and rail never disagree.

### Classification: answerable vs hand-off (D13, D36)
```
function dialog_actions(item, daemon, now):
    if stale or daemon not connected:   answer controls disabled, reason "daemon disconnected"; keep hand-off
    elif expired(item, now):            controls disabled, reason "expired — waiting for the agent"   # item stays until UiResolved
    elif item.kind == Confirm:          buttons [ "Yes" → {confirmed:true}, "No" → {confirmed:false} ]
    elif item.kind == Select and item.options non-empty:
                                        one button per option, in payload order, label = option string, response {value: option}
    else:  # Input, Editor, Select without options, Other
        hand_off("free-text answer needed" | "no options offered" | "unsupported dialog")   # primary action: Open in web UI
    all kinds: secondary action "Open in web UI" (agent.openInWebUi) when a URL is configured
    all buttons disabled while item.inFlight
```
`input`/`editor` (and prefill/placeholder) are never rendered for editing — hand-off only. Dismiss (`cancelled: true`) is **not** offered in v1 (TODO(verify): pi-studio's web UI also has no dismiss for `select`/`input`; confirm it isn't essential, e.g. for a dialog that can't otherwise be cleared). `optionDetails` descriptions are not rendered in v1.

Item content (shown in the app window only; D28, D32): agent title, daemon name, method, dialog `title` (≤ 200), `message` for confirm (≤ 500), option labels (≤ 50, ≤ 120 chars each). Rendered as plain text, never interpreted. None of these strings appear in notifications, toasts, logs or the event ring (D29, D32). Countdown: when `expiresAt` exists the item shows "expires in mm:ss" from the local clock (skew accepted, D28), switching to "expired" at zero.


### `dialog.respond` flow
```
dialog.respond(serverId, requestId, response):
    item = uiRequests[(serverId, requestId)]
    guard: exists, Answerable, !inFlight, !expired, daemon connected, response valid for item
           (confirm: has boolean `confirmed`; select: `value` in item.options)           # else error with reason, nothing sent
    item.inFlight = true; item.inFlightResponse = response                               # buttons disabled (D27)
    send_to_daemon(serverId, RespondUi{uiRequestId: requestId, response})                 # exactly {confirmed} or {value}; no other keys
    no RequestResult within 15 s → inFlight = false, Toast("no reply from <daemon>")      # TODO(verify) value
on RequestResult(RespondUi, ok:true):        remove item (idempotent with UiResolved); silent
on RequestResult(RespondUi, ok:false, error: "not_found"): remove item; Toast("no longer pending")       # D27, D37 — id unknown or already resolved
on RequestResult(RespondUi, ok:false, other error, e.g. "unsupported"): inFlight = false; Toast("answer failed: <error code>")
on UiResolved arriving while not in flight:  remove; Toast per reason (`answered` → "answered elsewhere" | `timeout` → "request timed out" | `aborted` → "request aborted" | other → "request <reason>"); notification marked resolved
on disconnect while in flight: inFlight = false; item stale, controls disabled until rebuild
```
The daemon forwards the answer to Pi; Pi ignores an answer for an already-resolved id (pi-studio `extension-ui-rpc.md`), so double clicks and simultaneous answers are harmless but are prevented anyway by the in-flight flag. The response never contains `id`/`type` keys (pi-studio stamps envelope fields after spreading; the dashboard sends none).

### `agent.interrupt` flow (D14, D27, D36)
```
agent.interrupt(key, confirm=false): guard enabled; frontend opens confirmInterrupt dialog:
    "Interrupt <agent title> on <daemon>? The running turn is stopped. This cannot be undone. Open questions from this agent stay pending."
    buttons [Interrupt (destructive)] [Cancel]
agent.interrupt(key, confirm=true):
    re-check status == running at confirm time; if not running: close dialog, Toast("agent is no longer running"), send nothing       # D27
    mark interruptInFlight; send_to_daemon(serverId, InterruptAgent{agentId})
on RequestResult(InterruptAgent, ok:true): clear flag; status change arrives via AgentUpdate / turn_canceled; pending dialogs REMAIN
on RequestResult(InterruptAgent, ok:false): clear flag; Toast("<agent> was not running")
```
No auto-confirm; Enter confirms only when the destructive button is focused.

### `agent.openInWebUi` (D24)
```
base = daemon_entry.webUiUrl                     # local default http://localhost:4173; http/https only, else disabled
host = ws(s)://<entry.host>:<entry.port>         # scheme from entry.tls
url  = base_without_query_or_fragment + "/?host=" + percent_encode(host) + "&connect=1" + "#agent=" + percent_encode(agentId)
open_url(url)   # system browser
```
Shipped by pi-studio sprint-082 (`web-ui-agent-deep-link.md`): the web UI never auto-connects from a bare link, so `host` and `connect=1` follow its query convention. The password is never placed in the URL; if the daemon needs one the user enters it in the web UI. Pairing fragments are not used by the dashboard.

### Toasts
Engine `Toast{text, level}` effects feed `UiState.toasts` (bounded 5). Texts contain agent title, daemon name or error codes only; never dialog/option/`action` text.

### Races (D27)
| Race | Result |
|------|--------|
| Dialog answered in web UI first | `UiResolved{answered}` while not in flight → item cleared, toast "answered elsewhere"; notification marked resolved |
| Simultaneous answers | Whichever the daemon accepts first wins; the loser gets `ok:false`/`not_found` → "no longer pending" |
| Click lands just after resolution | guard "item not found" → no-op, toast "no longer pending" |
| Dialog times out (`timeoutMs`) | controls disable at local expiry; item removed on `UiResolved{timeout}`; toast "request timed out" |
| Interrupt of stopped agent | no-op + message |
| Respond while disconnected/reconnecting | disabled; never queued |
| Item's agent archived/deleted | item cleared with the agent |

## Data & persistence touchpoints
- Reads `MonitorState.uiRequests`, agent `attention`, `daemons[].info`/`webUiUrl`; writes `inFlight` flags in `MonitorState`, `UiState.dialog` (`confirmInterrupt`), `UiState.toasts`.
- Outbound only via `EngineServices::send_to_daemon`; allowlist enforced in L0/L4.
- Nothing persisted (D8); no dismissal state.

## Error handling & edge cases
| Condition | Expected behavior |
|-----------|-------------------|
| `select` with empty/non-string options | hand-off ("no options offered") |
| `response.value` not in options (stale UI) | command rejected; nothing sent |
| Daemon returns `ok:false` with other error | inFlight cleared; toast with error code only |
| Dialog for an agent missing from the list | item shown with agent id as title |
| Stale daemon | items remain, flagged stale, answer controls disabled (D25) |
| Missing web UI URL | hand-off button disabled with reason; item cannot be answered here if hand-off |
| Unknown dialog `method` with `expectsResponse` | hand-off "unsupported dialog" |
| Interrupt while a dialog is pending | dialog stays pending (D36) |
| Huge `title`/`message`/options | capped at ingest (live-state) |
| Dialog without `timeoutMs` | stays until `UiResolved` |

## Dependencies on other specs
- [live-state.md](../architecture/live-state.md) — `UiRequestItem`, classification, reducers, effects.
- [protocol-client.md](../architecture/protocol-client.md) — `RespondUi`/`InterruptAgent`, allowlist, `uiPending` catch-up.
- [app-structure.md](../architecture/app-structure.md) — command table, `UiState.dialog`, `focusItem`.
- [notifications-and-tray.md](notifications-and-tray.md), [agent-monitor.md](agent-monitor.md).
- pi-studio `extension-ui-rpc.md`, `web-ui-agent-deep-link.md`.

## Acceptance criteria
- [ ] Given a `confirm` or `select` dialog pending before the dashboard connected, when it connects, then the dialog is in the attention rail (via `agent_ui_list_request`). (D36; brief 3)
- [ ] Given a pending `confirm`, when the engineer clicks Yes or No, then `agent_ui_respond_request {confirmed}` is sent, buttons are disabled while in flight, and the item clears here and in the web UI. (D13, D27, D36; brief 4)
- [ ] Given a pending `select` with options, then one button per option appears in payload order and clicking one sends `{value: <option>}`. (D13, D36)
- [ ] Given a dialog answered first in the web UI, then the dashboard item clears with toast "answered elsewhere". (D27; brief 5)
- [ ] Given a response racing another client, then the loser sees `{ok:false, error:"not_found"}` → item clears with toast "no longer pending" and no error state. (D27, D37)
- [ ] Given an `input` or `editor` dialog, or a `select` without options, then no answer controls are shown and "Open in web UI" opens `<webUiUrl>/?host=<ws(s)://host:port>&connect=1#agent=<urlencoded agentId>` in the system browser, with no password in the URL. (D13, D24, D36; brief 6)
- [ ] Given `agent_ui_request` with `expectsResponse:false` (setStatus/setWidget/notify) or `removed:true`, then no rail item appears. (D36)
- [ ] Given a dialog with `timeoutMs`, then the item shows a countdown, disables at expiry, and clears on `agent_ui_resolved{timeout}`; given `agent_ui_resolved{aborted}`, then it clears with toast "request aborted". (D36)
- [ ] Given a running agent, when the engineer interrupts and confirms, then `interrupt_agent` is sent, the agent goes idle, and its pending dialogs remain pending and answerable. (D14, D36; brief 7)
- [ ] Given the confirmation dialog, then it states the turn stops and cannot be undone and that open questions stay pending. (D14, D36)
- [ ] Given an agent that stopped while the dialog was open, when confirmed, then nothing is sent and "agent is no longer running" appears. (D27)
- [ ] Given a daemon entry with no web UI URL, then `agent.openInWebUi` is disabled with a reason and the engine refuses it. (D24)
- [ ] Given a `turn_failed`, then a failure item appears and clears on the next `turn_started`. (D1, D9)
- [ ] Given a disconnected daemon, then its items show stale and answer buttons are disabled with reason. (D25)
- [ ] Given a dialog `message` or option of 1 MB, then the model keeps at most the caps and nothing of it is logged or notified. (D28, D29, D32)
- [ ] A test enumerates command ids and `OutboundRequest` variants and proves only `agent_ui_respond_request` and `interrupt_agent` can mutate a daemon, and that no permission command or item kind exists. (D2, D37; brief 12)
- [ ] Given a dialog broadcast by the daemon, then it is visible in the rail within 500 ms. (D33)

## TODO(verify)
- Dismiss (`cancelled: true`): not offered; confirm it is not essential (no other way to clear a dialog from the dashboard besides answering or the web UI).
- Response timeout value (15 s).
- Bracketed IPv6 hosts in the deep link's `host` param (the shipped format is otherwise fixed: `?host=<ws(s)://host:port>&connect=1#agent=<urlencoded agentId>`).
- Whether `optionDetails` descriptions should be rendered.
