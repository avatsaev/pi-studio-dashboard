# Task 002 — `dialog.respond` command and outbound sweep

- **Sprint:** sprints/sprint-007-attention-interventions
- **Status:** backlog
- **Type:** feature
- **Area:** L5 engine
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-001, sprint-005-daemon-connections/task-001

## Goal
Let the engineer answer a confirm/select dialog through one command that sends exactly `agent_ui_respond_request` and handles every result and race.

## Context / why
The first of the dashboard's two daemon mutations (D2, D36). The engine, not the frontend, owns the guards (golden rule 5); in-flight state prevents double sends; results map to the toasts in the spec. This task also adds the sweep proving nothing else can mutate a daemon.

## Scope references
- `swe/specs/features/attention-and-interventions.md` § Commands, § `dialog.respond` flow, § Races, § Error handling & edge cases
- `swe/specs/architecture/app-structure.md` § Command registry, § Daemon-mutation guard, § Services hooks
- `swe/specs/architecture/protocol-client.md` § Requests the dashboard sends, § Mutation allowlist, § Outbound allowlist enforcement
- `crates/engine/src/` (new `dialog_cmds.rs`; register in `commands.rs`; params in `params.rs`)
- `crates/testkit/` (recording `send_to_daemon` fake; `adversarial_params` key for `dialog.respond`)
- `xtask` `panic_hunt` seed list
- `crates/protocol/tests/fixtures/pi-studio/` `agent_ui_respond_*.json`

## What to build
- Command `dialog.respond {serverId, requestId, response}` where `response` is `{confirmed: bool}` (confirm) or `{value: string}` (select; `value` MUST equal one of the item's `options`). Params strictly validated (unknown keys rejected, no `id`/`type` keys).
- Guards, each returning a typed error/disabled reason and sending nothing: item exists ("no longer pending"), `Answerable` ("answer in web UI"), not `inFlight` ("already sending"), not expired ("expired"), daemon connected and not stale ("daemon disconnected"), response valid for the item ("invalid option").
- Success: set `inFlight=true` + `inFlightResponse`; call `EngineServices::send_to_daemon(serverId, OutboundRequest::RespondUi{uiRequestId: requestId, response})`. Hook absent → disabled with a reason (never a panic).
- `RequestResult(RespondUi)`: `ok:true` → remove item, silent (idempotent with `UiResolved`); `ok:false, error:"not_found"` → remove item + `Toast("no longer pending")`, no error state; other error (e.g. `unsupported`) → `inFlight=false` + `Toast("answer failed: <error code>")` (code only).
- No result within 15 s → `inFlight=false` + `Toast("no reply from <daemon>")` (injected clock, testable).
- Disconnect while in flight → `inFlight=false`, item stale, controls disabled until rebuild. Mutations are never queued or replayed.
- Non-recording command; its enabled state/reason is exposed so the view's `actions[]` can use it (consumed by T005).
- **Registry sweep test** (engine or testkit): enumerate every registered command id and every `OutboundRequest` variant; assert the wire mutation set is exactly `{agent_ui_respond_request, interrupt_agent}` and that no command id or attention-item kind contains "permission". The `agent.interrupt` half is completed in T003.

## Out of scope
- `agent.interrupt` — sprint-007/task-003 (sweep extended there).
- `agent.openInWebUi` — sprint-007/task-004.
- Rail buttons and countdown rendering — sprint-007/task-005.
- Dismiss (`cancelled:true`) is not offered in v1.

## Acceptance criteria
- [ ] Given a pending confirm, when `dialog.respond {confirmed:true}` runs, then exactly one `agent_ui_respond_request` with `response {confirmed:true}` and the dialog's id as `uiRequestId` reaches the recording hook and the item is `inFlight`.
- [ ] Given a pending select with options [a,b], when `dialog.respond {value:"a"}` runs, then `{value:"a"}` is sent; given `{value:"c"}`, then nothing is sent and the reason is "invalid option".
- [ ] Given an item in flight, when `dialog.respond` runs again, then nothing is sent ("already sending").
- [ ] Given `RequestResult ok:false error:"not_found"`, then the item is removed with toast "no longer pending" and no error state; given error `unsupported`, then `inFlight` clears and the toast contains only the error code.
- [ ] Given an input/editor item, an expired item, a stale/disconnected daemon, or a missing item, then the command is refused with the matching reason and nothing is sent.
- [ ] Given no `RequestResult` for 15 s, then `inFlight` clears and the toast "no reply from <daemon>" appears.
- [ ] Given a disconnect while in flight, then `inFlight` clears and the item is stale with controls disabled.
- [ ] Given the sweep test, then only `agent_ui_respond_request` and `interrupt_agent` are mutation wire types and no permission command or item kind exists.

## Test / verification plan
- Tests: engine tests with the testkit recording hook and fake clock (each guard/result above); the sweep test. `cargo test -p pistudiodashboard-engine`.
- `panic_hunt` covers `dialog.respond` via adversarial params (add keys to the seed).
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- TODO(verify): the 15 s response timeout value (attention spec).
- The `cancelled:true` dismiss is intentionally absent (TODO(verify) whether it is essential).
