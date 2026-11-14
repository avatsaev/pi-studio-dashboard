# Task 001 — Dialog wire types and reducers

- **Sprint:** sprints/sprint-007-attention-interventions
- **Status:** backlog
- **Type:** feature
- **Area:** L0 protocol + L2 reducers
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** sprint-006-agent-monitor/task-002

## Goal
Decode Pi dialog frames, classify them into `UiRequestItem`s and reduce them into `MonitorState` with attention counts and effects.

## Context / why
Dialogs (`agent_ui_request` with `expectsResponse:true`) are the only answerable attention source (D36, D37). The reducers must create/clear items exactly per the table in the attention spec so the rail (T005) and the card flags can never disagree. Surfaces (`setStatus`, `notify`, `removed`) are never items. Nothing of a dialog's text may reach logs, the event ring, notifications or toasts (D29, D32).

## Scope references
- `swe/specs/features/attention-and-interventions.md` § What creates and clears items, § Classification, § Races
- `swe/specs/architecture/live-state.md` § UiRequestItem (state table), § Dialog classification (`from_wire`), § Per-event reducers (UiRequest/UiResolved/Snapshot), § Bounds and enforcement
- `swe/specs/architecture/protocol-client.md` § Messages the dashboard consumes
- `crates/protocol/src/` (add `ui.rs` or equivalent; `lib.rs` re-exports)
- `crates/model/src/` (`UiRequestItem`, caps)
- `crates/ingest/` (`pistudiodashboard-ingest`, the L2 reducer crate created in sprint-002-domain-cutover/task-001)
- `crates/protocol/tests/fixtures/pi-studio/` `agent_ui_request.*.json`, `agent_ui_resolved.*.json`, `agent_ui_respond_request.*.json`, `agent_ui_respond_response.*.json`, `agent_ui_list_response.json`

## What to build
**Wire types (L0, lenient, serialize + deserialize)**
- `agent_ui_request`: `{requestId, agentId, method: string, expectsResponse: bool, payload: {title?, message?, options?: string[], prefill?, placeholder?, ...}, surfaceKey?, removed?, timeoutMs?, createdAt (number|ISO → epoch ms)}`. Unknown fields tolerated; non-string options dropped.
- `agent_ui_resolved`: `{requestId, agentId, reason: answered|timeout|aborted|cancelled|Unknown}` (`cancelled` tolerated, schema-only).
- Outbound for later tasks: `RespondUi{uiRequestId, response: {confirmed:bool} | {value:string}}` encoded as `{type:"session", message:{type:"agent_ui_respond_request", requestId, uiRequestId, response}}` (the response carries **no** `id`/`type` keys); response frame `agent_ui_respond_response {requestId, payload:{ok, error?}}`. `OutboundRequest::RespondUi` exists from sprint-004/task-004; this task adds/validates the typed payload and the response decode if sprint-004 left it opaque.

**Model (L1)**
- `UiRequestItem` exactly as in live-state § UiRequestItem: `{serverId, requestId, agentId, method, kind: Confirm|Select|Input|Editor|Other, answerability: Answerable|HandOff, title ≤200, message ≤500 (confirm only), options ≤50 entries each ≤120 chars (+ optionsTruncated), timeoutMs?, createdAt, expiresAt? (local = raisedAt + timeoutMs), raisedAt, inFlight, inFlightResponse?}`. Prefill/placeholder/editor text is never retained.
- `from_wire(serverId, wire, now)`: `Confirm` → Answerable; `Select` with ≥1 string option → Answerable; `Select` without options, `Input`, `Editor`, unknown method → HandOff (reasons: "no options offered" / "free-text answer needed" / "unsupported dialog").
- Caps enforced at ingest (a 1 MB `message` or option is truncated before storing).

**Reducers (L2, pure)**
- `UiRequest`: ignore when `!expectsResponse` or `removed`; else idempotent upsert keyed `(serverId, requestId)`; newly inserted → effect `Notify{needsInput, serverId, itemId:"ui:<requestId>"}`; card `attention.pendingDialogs` recomputed.
- `UiResolved{reason}` for an item **not** in flight: remove; `Toast(info, …)` per reason (`answered` → "answered elsewhere", `timeout` → "request timed out", `aborted` → "request aborted", other → "request <reason>") + `ResolveNotification`.
- `Snapshot.uiPending`: rebuilt via `from_wire` (items omitted from the snapshot cleared; items already present kept idempotently, so no duplicate when the live broadcast also arrives). Snapshot items do **not** emit `Notify` (the first-sync baseline summary is sprint-008/task-001).
- Agent archived/deleted, daemon removed → its items cleared (no toast).
- Attention counts (`attention.pendingDialogs` per card; per-daemon and total counts used by status bar/tray) derived from `uiRequests`; stale-daemon items are kept and flagged stale.

All strings stay out of logs/events (counts and sizes only).

## Out of scope
- `dialog.respond` and `RequestResult` handling — sprint-007/task-002.
- `agent.interrupt` — sprint-007/task-003.
- `agent.openInWebUi` — sprint-007/task-004.
- Failure attention items from `turn_failed`/`error` status (reducers exist from sprint-006/task-002); rail rendering — sprint-007/task-005.
- Notification coalescing/toggles/baseline summary and the OS notify hook — sprint-008/task-001.

## Acceptance criteria
- [ ] Given fixtures `agent_ui_request.confirm/select/editor/notify/setStatus/setStatus.removed`, when decoded and reduced, then confirm and select produce Answerable items, editor produces a HandOff item, and notify/setStatus/removed produce none.
- [ ] Given a `select` without `options` or with only non-string options, then answerability is HandOff with reason "no options offered".
- [ ] Given `input`, `editor` and an unknown method with `expectsResponse:true`, then each is HandOff and no prefill/placeholder text is stored anywhere in the model.
- [ ] Given a confirm pending in `Snapshot.uiPending` and the same request arriving live afterwards, then exactly one `UiRequestItem` exists.
- [ ] Given `agent_ui_resolved` with `answered`, `timeout`, `aborted`, `cancelled` (and an unknown reason), then the item is removed, the toast is respectively "answered elsewhere", "request timed out", "request aborted", "request cancelled" and "request <reason>", and a `ResolveNotification` effect is returned each time.
- [ ] Given a dialog `message` or option of 1 MB, then the stored item respects the caps and no part of it appears in any event-ring entry, effect string or log output.
- [ ] Given an agent archived/deleted or a daemon entry removed, then its dialog items are cleared.
- [ ] Given `timeoutMs` 30000, then `expiresAt = raisedAt + 30000`; and `createdAt` accepts a number or an ISO string.
- [ ] Given a new live dialog, then one `Notify{needsInput, itemId:"ui:<requestId>"}` effect is returned; re-delivery of the same key returns none.

## Test / verification plan
- Tests: fixture decode for every `agent_ui_*` file via the vendored manifest (`crates/protocol/tests/`); reducer tests in the L2 crate; one proptest that arbitrary sequences of UiRequest/UiResolved/Snapshot never exceed caps or panic (plausible consumer-visible bugs: unbounded memory, panics).
- `cargo test -p pistudiodashboard-protocol -p pistudiodashboard-model -p pistudiodashboard-ingest`; all pass.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- TODO(verify) (attention spec): `optionDetails` descriptions are not rendered; ignore the field.
- Dialog `title` is the only payload text that may later leave the window (notification body ≤ 80, sprint-008); do not add other exfil paths.
