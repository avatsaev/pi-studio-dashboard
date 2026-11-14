# Task 002 — Agent reducers and activity

- **Sprint:** sprints/sprint-006-agent-monitor
- **Status:** done
- **Type:** feature
- **Area:** crates/ingest (L2)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-001, sprint-002-domain-cutover/task-001

## Goal
Add the agent reducers (update/archived/deleted/stream) with activity-summary derivation, the errored flag and failure notifications, keeping card order stable.

## Context / why
Turns the decoded events into the per-card summary (current tool, last output line, turn state, errored) the grid shows (D6, D9). Reducers are pure and bounded (D17, D28) and take `now` as an argument.

## Scope references
- swe/specs/architecture/live-state.md § Per-event reducers, § Activity summary derivation from `agent_stream` events, § Derivations, § Bounds and enforcement
- swe/specs/features/agent-monitor.md § Status dot mapping, § Card grid
- `crates/ingest/src/{reduce_agent.rs,activity.rs}` (new), `crates/ingest/src/lib.rs`
- `crates/model/src/` (`AgentEntry`, `ActivitySummary`, caps)

## What to build
In `crates/ingest` (pure; no clock, no I/O):
- `AgentUpdate(a, status?, title?, labels?, model?)` / `AgentStatus(a, status)`: if the agent is absent create a placeholder `{status ?? idle, title}` and add effect `FetchStats`; patch only present fields (absent = unchanged, `title:null` clears); `lastActivity = now` on status/title change; running → not-running clears `currentTool`; `after_status_change`: new `error` and old ≠ `error` → `attention.errored = true`, push `agentError` event record, effect `Notify{failures, serverId, itemId:"fail:<agentId>"}`; old `error` → not error clears `errored`.
- `AgentArchived(a)`: `archived = true`, drop its `uiRequests`, `CloseDrill("archived")` if it is the drill target (drill logic itself in task-005; emit the effect now); `AgentDeleted(a)`: remove entry and its `uiRequests`, `CloseDrill("deleted")`.
- `AgentStream(a, seq?, event)`: `update_activity(entry, event)` per the table — `turn_started` clears tool + last line, `turnState=started`, clears `lastErrorMessage`; `assistant_message` non-final updates `lastOutputLine` from the last non-empty line of a per-agent text tail buffer (≤ 1 KiB, reset on `final`/`turn_started`); `reasoning` never touches `lastOutputLine`; `tool_call` started/running (partial or not) sets `currentTool{callId, kind, label=toolDetailText ≤ 120 single line, status}` (most recently started/updated running call wins with parallel calls) and, for partial with non-empty `output`, `lastOutputLine` = last non-empty line (≤ 200); completed/error clears `currentTool` if `callId` matches and sets `lastOutputLine` from non-empty output; unknown status string = running; `turn_completed` → clear tool, `turnState=completed`; `turn_failed{error?}` → clear tool, `turnState=failed`, `lastErrorMessage` ≤ 200, `errored = true`, `Notify(failures)`; `turn_canceled` → clear tool, `canceled`; `error{message?}` → `lastErrorMessage`, push `agentError`, `Notify(failures)`, does NOT set `errored`; `compaction`/`queue_update`/`user_message` ignored; unknown kind → counted. `toolDetailText`: shell→`command`, read/write/edit→`path`, search→`query`, fetch→`url`, task→`description`.
- `turn_completed`/`turn_failed` additionally return `FetchStats{serverId, agentId}` (scheduling/dedupe is client-side in task-003; keep the effect for the `Effect` contract).
- `lastActivity` is NOT bumped by `agent_stream` (only by status/title changes and item raise) so `card_order` stays put while streaming. Drill-buffer application of stream events is task-005; the `drill_apply` call site is added there.
- Dialog reducers (`UiRequest`/`UiResolved`) and `RequestResult` are sprint-007; `Stats`/`Timeline` events are tasks 003/005.
- Derived strings carry the length caps of the bounds table; ring/event caps unchanged. Event records never include command/path/message text (`summary` is generic).
- Property tests (`proptest`): over arbitrary `DaemonEvent` streams (existing + new kinds) all bounds hold, `apply` never panics and is deterministic (same input → same output), `card_order` over the result is a total order (reflexive/antisymmetric/transitive via sort stability check) and identical for any delivery order of stream deltas.

## Out of scope
- Stats usage reducer (task-003); drill-down buffer (task-005); dialog reducers and attention counts from dialogs (sprint-007); notification policy (sprint-008).
- Card/grid view assembly (task-004).

## Acceptance criteria
- [ ] Given `turn_started`, `tool_call{running, shell, command}`, `tool_call{partial, output:"a\nb"}`, then `currentTool.label` is the command and `lastOutputLine == "b"`; `tool_call{completed}` clears `currentTool`; `reasoning` leaves `lastOutputLine` unchanged. (D6)
- [ ] Given `turn_failed`, then `turnState == failed`, `attention.errored == true` and exactly one failures `Notify` effect is returned; given `error{message}` only, then `lastErrorMessage` is set, `errored` stays false, and one failures `Notify` is returned. (D9)
- [ ] Given an `agent_update` that changes status idle→running→idle, then `currentTool` is cleared on the exit from running and `lastActivity` is bumped only by the status/title changes, not by stream events.
- [ ] Given an `agent_update` with `status:"error"`, then `errored` is true with one `Notify(failures)`; a later non-error update clears `errored`.
- [ ] Given an update for an unknown agent, then a placeholder entry exists and a `FetchStats` effect is returned; given an `agent_stream` for an unknown agent, then it is ignored and counted.
- [ ] Given `agent_archived`, then the entry is archived, its dialogs dropped and a `CloseDrill` effect is returned only if it was the drill target; given `agent_deleted`, the entry is removed.
- [ ] Given a status `closed`, then card rank is closed/last; given mixed statuses and `needs_attention`, `card_order` ranks attention → running → error → idle(initializing/unknown) → closed with tie-breaks (daemon order, lastActivity desc, title, agentId). (D15)
- [ ] Given any delivery order of stream deltas, then `card_order` is identical. (D15)
- [ ] Property: for arbitrary event sequences all bounds hold, reducers never panic, `apply` is deterministic and `card_order` is a total order. (D15, D17, D28)
- [ ] Given a 1 MB assistant text/title/error, then summary fields are within their caps (title ≤ 200, label ≤ 120, output line ≤ 200, error ≤ 200, tail ≤ 1 KiB).

## Test / verification plan
- `cargo test -p pistudiodashboard-ingest` including the proptest suite; add golden replay tests that feed the vendored `agent_stream.*`/`agent_update*` fixtures through `apply` (D22) and assert the summary table rows.
- `cargo xtask wasm` builds `ingest`.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (live-state): `queue_update` is named in the table although the vendored fixtures have no such kind — treat unknown/`queue_update` as ignored.
- Status sources: only `list_agents_response` and `agent_update.status` (D37); never rely on `agent_status`.
- `needs_attention` (dialogs) is fed by sprint-007; here only `errored` is set.
