# Task 001 — Agent wire types

- **Sprint:** sprints/sprint-006-agent-monitor
- **Status:** done
- **Type:** feature
- **Area:** crates/protocol (L0) + crates/client (L4)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** sprint-004-protocol-client/task-005

## Goal
Decode the agent push and stats/timeline messages in L0 and make the client emit the matching `DaemonEvent` kinds.

## Context / why
These are the messages that turn the daemon's push stream into cards (D6, D15, D16, D37). Decoding is lenient so a bad field never drops a whole update.

## Scope references
- swe/specs/architecture/protocol-client.md § Messages the dashboard consumes, § Lenient decoding rules, § Events emitted by L4
- swe/specs/architecture/live-state.md § `DaemonEvent`
- `crates/protocol/src/{agent.rs,stream.rs,stats.rs,timeline.rs}` (new)
- `crates/client/src/conn.rs` (push → event mapping)
- crates/protocol/tests/fixtures/pi-studio/{agent_update,agent_update.model,agent_status,agent_archived,agent_deleted,agent_stream.*,agent_timeline_reset,agent_session_stats_request,agent_session_stats_response,fetch_agent_timeline_request,fetch_agent_timeline_response}.json

## What to build
**L0 types** (all `Serialize + Deserialize`, lenient, unknown fields tolerated):
- `agent_update {agentId (req), status?: AgentStatus, title?: Option<Option<String>> (string|null; ≤ 256), labels?: map (partial; absent = unchanged), model?, modelProvider?}` — **partial**: the type must distinguish 'absent' from 'null' for `title`. Fixtures `agent_update.json` (status `initializing`, `labels:{}`) and `agent_update.model.json` (`model`, `modelProvider` only).
- `agent_status {agentId, status}` — decoded for tolerance, **never emitted** by any daemon path (fixture is a schema-only exception); emit an `AgentStatus` event if received.
- `agent_archived {agentId, archivedAt?: epoch_ms}` (ISO string → ms), `agent_deleted {agentId}`.
- `agent_stream {agentId, seq?: u64, timestamp?: epoch_ms, event: StreamEvent}`; `StreamEvent` discriminated by `event.kind`: `user_message{messageId?, text?}`, `assistant_message{text, final?: bool}`, `reasoning{text?, …}`, `tool_call{callId, status: String (free string: started|running|completed|error|canceled|…), partial?: bool, tool:{kind, command? | path? | query? | url? | description?, output?}}`, `turn_started{turnId?}`, `turn_completed{turnId?}`, `turn_failed{turnId?, error?}`, `turn_canceled{turnId?}`, `error{message?}`, `compaction{compactionId?, phase?, reason?}`, `Unknown{kind}` (counted, never fatal). A `tool_call` partial (`partial:true`) and any delta carry **no `seq`**. Long strings capped at decode (text/output ≤ 64 KiB; error ≤ 1 KB; titles ≤ 256).
- `agent_timeline_reset {agentId, reason?}` (open string; shipped `"fork"`).
- `agent_session_stats_response {requestId, payload:{ok?, sessionId?, totalMessages?, tokens?:{total, input?, output?, cacheRead?, cacheWrite?}, cost?: f64, contextUsage?:{tokens?, contextWindow?, percent?}, model?, error?}}` — every field beyond `tokens.total` optional (mock has no `cost`); `ok:false` means no session. `SessionStats{agentId}` request exists from sprint-004/task-004.
- `fetch_agent_timeline_response {requestId, agentId?, items: Vec<TimelineItem>, seqStart, seqEnd, hasNewer, startCursor?, endCursor?, collapsed?, sourceSeqRanges?}`; `TimelineItem {kind: String (projection label e.g. "other"), sourceSeq, timestamp: epoch_ms, event: StreamEvent}`; unrecognised items decode with `Unknown` event and are skipped later, never fail the page. Cursors are opaque strings (`"0"`, `"27"`).
**L4**: map each decoded push to the `DaemonEvent` kinds `AgentUpdate`, `AgentStatus`, `AgentArchived`, `AgentDeleted`, `AgentStream{agentId, seq?, timestamp?, event}`, `TimelineReset{agentId, reason?}`; responses to `SessionStats`/`FetchTimeline` complete the internal awaiter (the `Stats`/`Timeline` events come with sprint-006/task-003 and task-005). Pushes during `syncing` flow through the task-005 buffer (sprint-004) and are emitted after `Snapshot`. Unknown event kinds increment `unknownEnumValues`. Foldable-delta coalescing (50 ms window, same agent, no state change; never `tool_call` start/complete, `turn_*`, UI events; never for a drilled-in agent) is implemented here as the backpressure stage with `droppedEvents` counting only dropped foldable events and attention-class events never dropped (blocking reads instead).

## Out of scope
- Reducers using these events (task-002), stats scheduling and `Stats` events (task-003), the drill-down timeline loop and `Timeline` events (task-005).
- `agent_ui_request`/`agent_ui_resolved` (sprint-007/task-001).
- Stage-2 types (sprint-010).

## Acceptance criteria
- [ ] Given each `agent_stream.*.json` fixture (user_message, assistant_message, reasoning, tool_call, tool_call.partial, turn_started, turn_completed, turn_failed, turn_canceled, error, compaction), then it decodes to the matching `StreamEvent` variant with `seq` present except for the partial tool_call.
- [ ] Given `agent_update.json` and `agent_update.model.json`, then absent fields stay `None` ('unchanged') and `title:null` is distinguishable from an absent title.
- [ ] Given a status `hyperspace` or an unknown stream `event.kind`, then it decodes as `AgentStatus::Unknown` / `StreamEvent::Unknown{kind}`, increments `unknownEnumValues`, and the connection and other messages are unaffected. (D26)
- [ ] Given `agent_status.json`, then it decodes (tolerance) and the client emits an `AgentStatus` event; given `agent_timeline_reset.json`, then `TimelineReset{reason:"fork"}`.
- [ ] Given `agent_archived.json` with an ISO `archivedAt`, then it decodes to epoch ms; given an unparsable string, then `None`.
- [ ] Given the stats fixture (`tokens:{total}` only, no cost) and a response with `payload.ok:false`, then both decode without error.
- [ ] Given `fetch_agent_timeline_response.json`, then items, `seqEnd`, `hasNewer:false` and string cursors are exposed; an item with an unknown `event.kind` does not fail the page.
- [ ] Given a stalled event consumer and 50 k foldable deltas from the fake daemon, then client memory stays bounded, `droppedEvents` counts only foldable deltas, and no `turn_*`/tool start/complete event is dropped. (D17, D28)

## Test / verification plan
- `cargo test -p pistudiodashboard-protocol` (fixture decode tests via the testkit loader, volatile/exception honoured) and `cargo test -p pistudiodashboard-client` (event mapping, coalescing/backpressure test with the fake daemon).
- `cargo xtask wasm` still builds `protocol`.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): the exact foldable-delta coalescing rule is shared with app-structure ('align names'); implement per protocol-client § Backpressure.
- `agent_stream` `reasoning` fixture field names (text/delta) — read `agent_stream.reasoning.json` before fixing the struct.
- Cost has no unit/currency in pi-studio (`agentSessionStatsResponseSchema`): decode as a bare number.
