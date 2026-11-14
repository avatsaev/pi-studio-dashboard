# Task 004 — Outbound allowlist and request correlation

- **Sprint:** sprints/sprint-004-protocol-client
- **Status:** done
- **Type:** feature
- **Area:** crates/protocol (L0) + crates/client (L4)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-003

## Goal
Extend the minimal `OutboundRequest` enum that sprint-002/task-002 created (`SessionStats`, `FetchTimeline`) into the closed enum of read RPCs + exactly two mutations, and add the `MUTATION_ALLOWLIST` table, request/response correlation with timeouts and `rpc_error` mapping, and `RequestResult` events.

## Context / why
The daemon has no role gating, so the dashboard is the only guard (D2/D36/D37): nothing but `agent_ui_respond_request` and `interrupt_agent` may be expressed as a mutation. All later features send requests through this layer.

## Scope references
- swe/specs/architecture/protocol-client.md § Requests the dashboard sends, § Mutation allowlist, § Request/response correlation, § Mutations, § Outbound allowlist enforcement
- `crates/protocol/src/outbound.rs` (new), `crates/protocol/tests/outbound_allowlist.rs` (new)
- `crates/client/src/{requests.rs,conn.rs,handle.rs}`
- crates/protocol/tests/fixtures/pi-studio/{list_agents_request,agent_ui_list_request,agent_session_stats_request,fetch_agent_timeline_request,agent_ui_respond_request.confirm,agent_ui_respond_request.select,agent_ui_respond_response.ok,agent_ui_respond_response.not_found,interrupt_agent,interrupt_response,rpc_error}.json

## What to build
**L0 (`crates/protocol/src/outbound.rs`, created minimal by sprint-002/task-002 — extend it, don't add a second type)** — `OutboundRequest` becomes the closed enum:
`Ping`, `ListAgents`, `ListUi{agent_id?}`, `SessionStats{agent_id}`, `FetchTimeline{agent_id}` (one logical load; the client pages `before` then `after` itself, D39 — the wire `fetch_agent_timeline_request` carries `agentId, direction, limit` and `cursor` only when present), `RespondUi{ui_request_id, response: UiResponse}` with `UiResponse = Confirm{confirmed: bool} | Select{value: String}`, and `InterruptAgent{agent_id}`. (Stage-2 variants `DaemonStatus*`/`DaemonLogs*` are added by sprint-010/task-001; leave the enum open to additive variants only through the exhaustive-match mechanism below.)
- `fn wire_type(&self) -> &'static str` — exhaustive match (adding a variant without classifying it is a compile error); `fn class(&self) -> RequestClass { Read | Mutation }`; `pub const MUTATION_ALLOWLIST: [&str; 2] = ["agent_ui_respond_request", "interrupt_agent"]`; `class()` is `Mutation` iff `wire_type ∈ MUTATION_ALLOWLIST`.
- The only encode function takes `&OutboundRequest` plus a `request_id` and is crate-private to what L4 can call through one public `encode(&OutboundRequest, &str) -> Vec<u8>`; there is no generic send-raw-JSON API anywhere. All variants except `Ping` are wrapped `{type:"session", message:{type, requestId, …}}`; `Ping` is top-level. Wire shapes exactly as the fixtures: `RespondUi` → `{type:"agent_ui_respond_request", requestId, uiRequestId, response}` with `response` exactly `{confirmed}` or `{value}` (no other keys); `InterruptAgent` → `{type:"interrupt_agent", requestId, agentId}`; `FetchTimeline` carries `agentId, direction, limit` and `cursor` only when present.
- Response types: `RespondUiResponse` from `agent_ui_respond_response {payload:{ok, error?}, requestId}` (`ok:false, error:"not_found"` is a domain result; other `error` strings stay open strings), `InterruptResponse {agentId, ok, requestId}` from `interrupt_response` (flat, session message).
- `all_variants()` helper (test-visible) with an exhaustive `match` guard so a new variant forces the test list to update. `tests/outbound_allowlist.rs`: (1) encodes every variant and asserts `wire_type` ∈ read set ∪ `MUTATION_ALLOWLIST` and `class()==Mutation` for exactly the two allowlisted; (2) scans the `protocol` crate's non-test source for string literals of known mutating pi-studio types (`delete_agent`, `archive_agent`, `send_agent_prompt`, `kill_terminal`, `create_agent_request`, `subscribe_terminal_request`, and any `permission` wire type) and fails if any appears outside tests/fixtures; (3) asserts no `permission` wire type exists in L0.

**L4 (`crates/client`)** — request path and correlation:
- `ClientHandle::request(entry_id|server_id, OutboundRequest, token) -> Result<(), SendError>` enqueues on the per-daemon bounded command channel with non-blocking `try_send`; connection not `live` → `Err(NotConnected)` immediately, **mutations are never queued or replayed after reconnect** (D27). The connection task accepts `OutboundRequest` and nothing else; the raw sink stays private to it.
- `newRequestId() = "<entryId-short>-<monotonic counter>"` unique per connection. Pending table `requestId → {kind, token, deadline}` bounded at 256; beyond that non-mutation requests fail fast with `Busy`. Timeouts: 10 s default, stats and timeline 15 s, mutations 15 s (dialog/interrupt). Completion on a session message whose `requestId` is pending; on `rpc_error` with a pending `requestId` → `Err(RpcError{code,message})`; on connection loss → `Err(ConnectionLost)` for all pending; on timeout → `Err(Timeout)` and the late response is dropped and counted `lateResponses`.
- `RequestResult{token, result}` event for `RespondUi` (`Ok | NotFound | Rejected{error} | NotConnected | Timeout | ConnectionLost`) and `InterruptAgent` (`ok: bool` or the same transport errors). Read-request results are delivered to internal awaiters (used by sync, stats, timeline in later tasks), not as `RequestResult` events. `token` is an opaque caller-supplied correlation value (the engine's in-flight key, sprint-007).
- A message with a `requestId` that is not pending is a push if its type is a known push, else unknown.

## Out of scope
- Concrete use of the mutations by commands (`dialog.respond`, `agent.interrupt`: sprint-007/task-002, -003) and the registry-sweep test (sprint-007/task-002).
- Sync sequence (task-005), stats scheduling (sprint-006/task-003), timeline paging (sprint-006/task-005).
- Stage-2 request variants (sprint-010/task-001).
- The `send_to_daemon` hook and app wiring (sprint-005/task-001).

## Acceptance criteria
- [ ] Given every `OutboundRequest` variant, when encoded, then `wire_type` is a read type or in `MUTATION_ALLOWLIST` = exactly {`agent_ui_respond_request`, `interrupt_agent`}, and only those two are `Mutation`; the source scan finds no other mutating type literal and no `permission` wire type. (D2, D36, D37)
- [ ] Given the encoded `RespondUi` for confirm and for select, then the JSON equals the frames in `agent_ui_respond_request.confirm.json`/`.select.json` modulo the volatile pointers, and `response` has no keys beyond `confirmed` / `value`.
- [ ] Given a fake daemon, when a client method issues a request that gets no answer for 10 s (simulated), then it fails with `Timeout` and a later response increments `lateResponses`.
- [ ] Given `agent_ui_respond_response {ok:false, error:"not_found"}`, then `RequestResult` is `NotFound`, not an error; given `{ok:false, error:"boom"}`, then `Rejected{error:"boom"}`; given `{ok:true}`, then `Ok`. (D37)
- [ ] Given `interrupt_response {ok:false}`, then `RequestResult` carries `ok:false` (domain result, not an error).
- [ ] Given an `rpc_error` with a pending `requestId`, then the request completes with `RpcError{code, message}`.
- [ ] Given the connection drops with a mutation pending, then `RequestResult{ConnectionLost}` is emitted and the request is never resent after reconnect; given `RespondUi` while not `live`, then `NotConnected` immediately with nothing queued. (D27)
- [ ] Given 300 outstanding non-mutation requests, then those beyond 256 fail fast with `Busy`.
- [ ] Given a fake daemon, when every public client method is driven, then the set of message `type`s the daemon received is a subset of the allowlist (read set + two mutations).

## Test / verification plan
- `cargo test -p pistudiodashboard-protocol --test outbound_allowlist` and `cargo test -p pistudiodashboard-client`; fixture round-trip for each request/response listed above using the testkit loader.
- `cargo xtask wasm` still builds `protocol`.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): whether `interrupt_response` is absent from pi-studio's Zod protocol union (cosmetic; the fixture pins the frame).
- The request timeout for mutations (15 s here) mirrors attention-and-interventions.md's 15 s `inFlight` reset (TODO(verify) value there).
- `direction` serialises as lowercase `before`/`after`; `limit` for the drill-down is 200 (sprint-006/task-005 sets it).
