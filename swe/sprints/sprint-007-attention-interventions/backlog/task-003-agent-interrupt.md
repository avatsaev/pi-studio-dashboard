# Task 003 — `agent.interrupt` with confirmation

- **Sprint:** sprints/sprint-007-attention-interventions
- **Status:** backlog
- **Type:** feature
- **Area:** L5 engine + L6 UiState
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Add the second (and last) daemon mutation: interrupting a running agent behind a mandatory confirmation, leaving pending dialogs untouched.

## Context / why
Interrupt is destructive (D14), so the engine refuses `confirm != true` and re-checks status at confirm time (D27). pi-studio's interrupt touches no pending dialogs, so the dashboard must not clear them either (D36).

## Scope references
- `swe/specs/features/attention-and-interventions.md` § Commands, § `agent.interrupt` flow, § Races
- `swe/specs/architecture/app-structure.md` § UiState (`dialog`), § Command registry, § Daemon-mutation guard
- `swe/specs/architecture/protocol-client.md` § Mutation allowlist
- `crates/engine/src/` (new `interrupt_cmds.rs` or extend `dialog_cmds.rs`; `commands.rs`)
- `crates/shell/src/state.rs` (`UiState.dialog` variant `confirmInterrupt{serverId, agentId}`)
- `crates/testkit/`
- `crates/protocol/tests/fixtures/pi-studio/` `interrupt_agent.json`, `interrupt_response.json`

## What to build
- Command `agent.interrupt {serverId, agentId, confirm: bool}`.
  - `confirm:false` → no daemon call; sets `UiState.dialog = confirmInterrupt{serverId, agentId}` (guards below apply first).
  - `confirm:true` → re-check agent status is `running` (or `initializing`); if not: close the dialog, `Toast("agent is no longer running")`, send nothing. Else mark `interruptInFlight` and `send_to_daemon(serverId, OutboundRequest::InterruptAgent{agentId})` (`{type:"session", message:{type:"interrupt_agent", requestId, agentId}}`).
  - The engine refuses to send without `confirm:true`.
- Enabled only when: agent `running`/`initializing`, daemon connected and not stale, no interrupt in flight. Reasons: "not running", "daemon disconnected", "interrupt in progress".
- `interrupt_response {agentId, ok, requestId}` (flat, no payload wrapper): `ok:true` → clear flag; status change arrives via `AgentUpdate`/`turn_canceled`; **pending dialogs are not cleared** (D36). `ok:false` → clear flag + `Toast("<agent title> was not running")`.
- Disconnect while in flight → clear flag. Extend the T002 sweep so it asserts both mutation commands exist and are the only ones.
- Confirmation copy is rendered by the frontend in T005 from `UiState.dialog`; the engine only stores the target.

## Out of scope
- Frontend confirm dialog and rail — sprint-007/task-005.
- `dialog.respond` — sprint-007/task-002.
- Any other agent action (archive, delete, prompt) is never added (D2).

## Acceptance criteria
- [ ] Given a running agent, when `agent.interrupt {confirm:false}` runs, then nothing is sent and `UiState.dialog` is `confirmInterrupt` for that agent.
- [ ] Given the confirm dialog open, when `agent.interrupt {confirm:true}` runs on a still-running agent, then exactly one `interrupt_agent {agentId}` is sent and the in-flight flag is set.
- [ ] Given the agent went idle while the dialog was open, when confirmed, then nothing is sent, the dialog closes and the toast reads "agent is no longer running".
- [ ] Given an idle or closed agent, or a disconnected daemon, then the command is disabled with the matching reason.
- [ ] Given `interrupt_response ok:true` while the agent has two pending dialogs, then both items remain pending and answerable; given `ok:false`, then the flag clears and the toast "<agent> was not running" appears.
- [ ] Given the extended sweep test, then both mutation commands are present and no other command can mutate a daemon.

## Test / verification plan
- Tests: engine tests with recording hook; shell test for `UiState.dialog` serialisation. `cargo test -p pistudiodashboard-engine -p pistudiodashboard-shell`.
- `panic_hunt` seed/adversarial keys for `agent.interrupt`.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- Spec: no auto-confirm; Enter confirms only when the destructive button is focused (frontend, T005).
- `interrupt_response` has no Zod schema in pi-studio; decode leniently (missing `ok` → failure).
