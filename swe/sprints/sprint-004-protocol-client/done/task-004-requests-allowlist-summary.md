# Task 004 — Outbound allowlist and request correlation — Summary

- **Sprint:** sprints/sprint-004-protocol-client
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
- **L0** `outbound.rs` extended (no second type): `OutboundRequest` = `Ping`, `ListAgents`, `ListUi{agent_id?}`, `SessionStats`, `FetchTimeline` (one logical load; encodes the first page `before`/200), `FetchTimelinePage{agent_id, cursor?, direction, limit}` (the client's own paging step), `RespondUi{ui_request_id, UiResponse::{Confirm,Select}}`, `InterruptAgent`. Exhaustive `wire_type()`, `class()` (`Mutation` iff in `MUTATION_ALLOWLIST = [agent_ui_respond_request, interrupt_agent]`), `READ_TYPES`, the single `encode(&OutboundRequest, request_id) -> String`, `all_variants()` with an exhaustive-match guard test. `reply.rs`: `RespondUiReply`, `InterruptReply`.
- **Model** (needed so the client can emit the result; additive): `DaemonEventKind::RequestResult{token, op: MutationOp, result: MutationResult}` with `MutationResult = Ok | NotFound | Rejected{error} | Interrupt{ok} | RpcError{code,message} | NotConnected | Timeout | ConnectionLost`; `ingest/apply.rs` gets a no-op arm (reducer is sprint-007/task-002).
- **L4** `requests.rs` + `conn.rs` + `handle.rs`: `ClientHandle::request(entry_id | server_id, OutboundRequest, token) -> Result<(), SendError{NotConnected, Busy, QueueFull}>` (non-blocking `try_send` on a bounded per-connection command channel that exists only while `live`; nothing is queued or replayed). Pending table `<entry8>-<n>` ids with timeouts 10 s (reads) / 15 s (stats, timeline, mutations); completion by `requestId`; `rpc_error` → `RpcError`; timeout → `Timeout` and the late answer counts `lateResponses`; connection loss → `ConnectionLost` for pending and queued mutations; 256 outstanding reads → `Busy`. Read results are internal (no event) until stats/timeline tasks.
- Spec: `protocol-client.md` request table fixed (FetchTimeline is one logical load; paging is `FetchTimelinePage`).

## Files created / changed
`crates/protocol/src/{outbound,reply,lib}.rs`, `crates/protocol/tests/outbound_allowlist.rs`; `crates/model/src/{event,lib}.rs` (+RequestResult types); `crates/ingest/src/apply.rs` (+1 no-op arm); `crates/client/src/{requests,conn,handle,lib}.rs`, `crates/client/tests/{requests.rs,support/*}`; `swe/specs/architecture/protocol-client.md`.

## How it satisfies the scope / deviations
`encode` returns `String` (a text frame) instead of `Vec<u8>`. `FetchTimelinePage` is an extra variant so paging needs no raw-JSON path. `NotConnected` is returned synchronously as `Err` (no event is emitted for it; the engine maps the `Err`). The `OutboundRequest::SessionStats/FetchTimeline` shapes the engine uses are unchanged; `cargo build --workspace` and the engine tests still pass.

## Build & test results
```
$ cargo test -p pistudiodashboard-protocol -p pistudiodashboard-client -p pistudiodashboard-model -p pistudiodashboard-ingest
122 passed, 0 failed, 4 ignored (live smoke)
$ cargo clippy ... --all-targets -- -D warnings   # clean;  cargo fmt --check clean
$ cargo check -p pistudiodashboard-protocol -p pistudiodashboard-model --target wasm32-unknown-unknown   # ok
$ cargo build --workspace   # ok;  cargo xtask layers -> layering: ok (11 crates)
```

## Acceptance criteria
- [x] Every variant: read type or allowlist; only the two are `Mutation`; source scan finds no other mutating literal and no `permission` type (outbound_allowlist.rs)
- [x] `RespondUi` confirm/select equal the fixtures, `response` has only `confirmed` / `value` (respond_ui_matches_the_fixtures…)
- [x] No answer → `Timeout` (mutation, 15 s simulated) and late read answer → `lateResponses` (unanswered_mutation_times_out…)
- [x] `{ok:false,error:"not_found"}` → `NotFound`; `"boom"` → `Rejected{boom}`; `{ok:true}` → `Ok` (respond_ui_results_and_tokens)
- [x] `interrupt_response {ok:false}` → `Interrupt{ok:false}` (interrupt_result_carries…)
- [x] `rpc_error` with pending id → `RpcError{code,message}` (respond_ui_results_and_tokens, t-rpc)
- [x] Drop with mutation pending → `ConnectionLost`, never resent; `RespondUi` while not live → `Err(NotConnected)`, nothing queued (pending_mutation_is_lost…, requests_while_not_live…)
- [x] 300 reads → 256 accepted, the rest `Busy`, the daemon sees exactly 256 (+1 sync) (reads_beyond_256…)
- [x] Every variant driven: received types ⊂ hello + read set + 2 mutations (every_variant_sent_stays_inside_the_allowlist)
- Fixture round-trips use direct file reads (protocol is `Standalone`, no testkit dependency).

## Smoke against pi-studio
Dev daemon 127.0.0.1:6790, `live_daemon_snapshot_and_mutations` (see task-005 summary for the Snapshot half): `RespondUi{confirm}` on a `#ui confirm` agent created for the purpose → `UiResolved{reason:"answered"}` push then `RequestResult{respond-1, Ok}`; the same request again → `RequestResult{respond-2, NotFound}`; `InterruptAgent` on a running `#ui select` agent → `RequestResult{interrupt-running, Interrupt{ok:true}}`; on an idle agent → `Interrupt{ok:true}` too (the mock daemon acknowledges). Only these two mutation types were sent.

## Follow-ups
- `RequestResult` and `UiResolved` need reducers (sprint-007). Timeouts of reads are not surfaced (no consumer yet).
- Interrupt of an idle agent returning `ok:true` on the mock: the "not running" toast (live-state.md) keys off `ok:false`; verify against a real provider.
