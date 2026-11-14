# Task 005 — Sync on connect

- **Sprint:** sprints/sprint-004-protocol-client
- **Status:** done
- **Type:** feature
- **Area:** crates/protocol (L0) + crates/client (L4)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-004

## Goal
Decode `list_agents_response` and `agent_ui_list_response`, run the sync-on-connect sequence and emit a `Snapshot` event, replaying buffered pushes after it.

## Context / why
After the handshake the dashboard re-lists agents and pending dialogs so state is rebuilt from the daemon's truth on every (re)connect (D25), and a push that raced the list must not be lost or resurrect a resolved dialog (D36).

## Scope references
- swe/specs/architecture/protocol-client.md § Sync-on-connect sequence, § Messages the dashboard consumes (list_agents_response, agent_ui_list_response), § Events emitted by L4 (Snapshot)
- swe/specs/architecture/live-state.md § Rebuild-on-connect (consumer of `Snapshot`)
- `crates/protocol/src/{sync.rs}` (new), `crates/client/src/sync.rs` (new) and `conn.rs`
- crates/protocol/tests/fixtures/pi-studio/{list_agents_response,agent_ui_list_response}.json
- `crates/protocol/tests/fixtures/local/` (degraded cases)

## What to build
**L0 types** (lenient, `Serialize + Deserialize`):
- `ListAgentsResponse {requestId, agents: Vec<ListedAgent>}` — flat (no `payload` wrapper). `ListedAgent {agentId (req), status: AgentStatus, title?: String (≤ 256), cwd?, labels: map, lastActivity?: epoch_ms, provider?, model?, modelProvider?, thinkingLevel?}`. `AgentStatus = Initializing|Idle|Running|Error|Closed|Unknown` (unknown wire value → `Unknown`, counts `unknownEnumValues`). A wrong type for an optional field decodes as `None`; an agent without `agentId` is dropped and counted `malformed` but the rest of the list survives.
- `UiListResponse {requestId, payload:{ok, pending: Vec<WireUiRequest>, surfaces?: Vec<…>, error?}}`. `WireUiRequest {requestId, agentId, method: String, expectsResponse: bool, payload: Value-preserving-preview, surfaceKey?, removed?, timeoutMs?, createdAt: epoch_ms}` — this is the same shape sprint-007/task-001 uses for the live push; define it here as the shared struct with the decode-time preview truncation applied (every string field ≤ 2048 bytes on a UTF-8 boundary, option lists ≤ 100 entries, `truncated:true` marker; the full payload is dropped, never stored or logged, D29). `surfaces[]` are decoded leniently and discarded (never attention items).

**L4 sync** (`crates/client/src/sync.rs`): after `Hello`, in state `syncing`, issue `ListAgents` (required) and `ListUi{}` concurrently, each with a 10 s timeout. `ListUi` failure (timeout, `rpc_error`, `payload.ok:false`) → warn in diagnostics `lastError`, empty `uiPending`; `ListAgents` failure → close, backoff (`bad_handshake`/`timeout`). While syncing, push messages (anything decoded as a known push) are buffered in a bounded queue (≤ 10,000 events; overflow drops the oldest foldable pushes first and counts `droppedEvents`, never the attention-class ones). When both complete: emit `ConnState{live}` then `Snapshot{agents, uiPending}` where `uiPending` = `pending[]` entries with `expectsResponse && !removed`; then flush the buffered pushes as individual events in arrival order. (At this point only unknown pushes can exist — the typed push events arrive in sprint-006/007 — so the buffer passes raw decoded pushes through a pluggable `PushSink`; unknown ones are counted.) Sync sends exactly `list_agents_request` then `agent_ui_list_request` (stats come later, sprint-006/task-003); the permission-list step does not exist (D37).
- `Snapshot` event shape: `{agents: Vec<ListedAgent>, uiPending: Vec<WireUiRequest>}` carrying the same `connEpoch`/`seq` header; the first `Snapshot` after a reconnect REPLACES the reducer's state for that daemon (reducer is sprint-002/task-001).

## Out of scope
- Typed push events `agent_update`, `agent_archived`, `agent_stream`, … (sprint-006/task-001) and `agent_ui_request`/`agent_ui_resolved` (sprint-007/task-001): until then they decode as unknown/raw and are only buffered and counted.
- Stats scheduling (sprint-006/task-003).
- The reducer/rebuild logic (sprint-002/task-001).
- Stage-2 resubscription after sync (sprint-010).

## Acceptance criteria
- [ ] Given `list_agents_response.json` and `agent_ui_list_response.json`, when decoded, then agent fields and the pending `select` dialog (`options:[a,b]`, `expectsResponse:true`, `createdAt`) round-trip with the fixtures' volatile pointers ignored.
- [ ] Given a list whose first agent has `status:"hyperspace"`, `title:42` and a second valid agent, then the first decodes with `status Unknown`, `title None` and the second is intact, and `unknownEnumValues` is 1. (D26)
- [ ] Given any daemon, when connecting, then the types of requests received are exactly `list_agents_request`, `agent_ui_list_request` (and nothing else before `Snapshot`), and `Snapshot` is emitted before any buffered push event. (D36)
- [ ] Given a push `agent_ui_resolved` received between the two requests and before `agent_ui_list_response`, then it is emitted AFTER the `Snapshot` (so the reducer removes the item instead of the stale list resurrecting it). (D27, D36)
- [ ] Given `agent_ui_list_request` times out or errors, then `Snapshot.uiPending` is empty, the connection still reaches `live`, and a warning is recorded; given `list_agents_request` fails, then the client backs off.
- [ ] Given a pending request with a 1 MB `message` string and 500 options, then the decoded preview has strings ≤ 2048 bytes (UTF-8 boundary safe), ≤ 100 options and `truncated:true`. (D28, D29)
- [ ] Given a pending entry with `expectsResponse:false` or `removed:true`, then it is not in `uiPending`.

## Test / verification plan
- `cargo test -p pistudiodashboard-protocol` (fixture decode + truncation unit tests incl. multi-byte boundary) and `cargo test -p pistudiodashboard-client` (sync ordering with the fake daemon, race test).
- `cargo xtask wasm` still builds `protocol`.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): `modelProvider` is absent when the daemon has none (mock); never required.
- Lists only NON-archived agents (`AgentManager.list()` filters `archivedAt`); an `agent_archived` push is what removes a card later (sprint-006).
- `agent_ui_list_request.agentId` is optional; sync always sends it without.
