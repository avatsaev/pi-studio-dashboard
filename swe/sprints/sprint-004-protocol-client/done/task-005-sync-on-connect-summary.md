# Task 005 — Sync on connect — Summary

- **Sprint:** sprints/sprint-004-protocol-client
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
- **L0** `sync.rs`: `AgentStatus` (Unknown fallback), `ListedAgent`, `ListAgentsResponse::from_message` (flat; missing `agentId` → dropped + `malformed`, unknown/missing status → `Unknown` + `unknown_status`, wrong-typed optionals → `None`, title ≤256 B), `WireUiRequest::from_value` with the decode-time preview (every string ≤2048 B on a char boundary, arrays/objects ≤100 entries, depth ≤8, `truncated` marker, full payload dropped), `UiListResponse::from_message` (surfaces never decoded). `push.rs`: `UiResolved`.
- **Model**: `DaemonEventKind::UiResolved{request_id, agent_id, reason}` (spec-listed event; no-op reducer arm in ingest, real reducer is sprint-007/task-001).
- **L4** `sync.rs` + `conn.rs`: after `Hello`, state `syncing` sends exactly `list_agents_request` and `agent_ui_list_request` (10 s each). Known pushes arriving meanwhile go to a bounded `PushBuffer` (≤10,000; overflow sheds the oldest foldable `agent_stream`/`daemon_log_lines` first, counts `droppedEvents`, never attention-class). Both done → `ConnState{live}` then `Snapshot{agents, ui_pending}` (only `expectsResponse && !removed`) then the buffered pushes are replayed (so far `agent_ui_resolved` → `UiResolved`; typed events for the rest arrive with sprints 006/007). `agent_ui_list_request` timeout / `rpc_error` / `ok:false` → empty `ui_pending`, connection stays live, warning on the `Live` state's `lastError` (+ `log::warn`). `list_agents` timeout → `timeout`, rpc error/malformed → `bad_handshake`, back off. Unknown statuses and dropped entries feed `unknownEnumValues` / `malformed`.
- Test seam: `ClientConfig.ping_every` and `.reset_after_live` (defaults 15 s / 10 s).

## Files created / changed
`crates/protocol/src/{sync,push,lib}.rs`, `crates/protocol/tests/sync.rs`; `crates/model/src/{event,lib}.rs`; `crates/ingest/src/apply.rs`; `crates/client/src/{sync,conn,requests,handle}.rs`, `crates/client/tests/{sync.rs,live_daemon.rs}` and fake-daemon scripting (`AgentsReply`, `UiReply`, `push_before_ui_list`, `Responder`, `push`).

## How it satisfies the scope / deviations
Event order is `Syncing`, `Live`, `Snapshot` (spec: "emit live then Snapshot"); requests are only accepted once live. Labels reach the model as `key=value` strings. Statuses reach the model as the wire string (`unknown` for unknown). `UiResolved` was added although typed dialog events are sprint-007 scope, because the acceptance race test needs an observable replay. Test-infrastructure finding: a paused tokio clock combined with real sockets lets a timer expire in the same wake-up as socket readiness (the runtime only auto-advances when idle), so the paused-clock tests that waited on replies (liveness pings, reset-after-live) now use real time with short configured periods; the client `select!`s are `biased` towards data over timers.

## Build & test results
Same commands as task-004 (122 passed, 0 failed, 4 ignored; clippy/fmt clean; wasm check of protocol and model ok; `cargo build --workspace` ok; layers ok). Client test files run 3× in a row without a failure.

## Acceptance criteria
- [x] Fixtures `list_agents_response.json` / `agent_ui_list_response.json` decode (agent fields; select dialog with options `[a,b]`, `expectsResponse`, `createdAt`) (protocol `tests/sync.rs`)
- [x] `status:"hyperspace"`, `title:42` + a valid second agent → `Unknown`, `None`, second intact, `unknownEnumValues == 1` (`list_survives_bad_entries`, `snapshot_is_built_from_both_lists…`)
- [x] Requests before `Snapshot` are exactly the two list requests; Snapshot precedes buffered pushes (`snapshot_is_built…`, `a_push_that_raced_the_list…`)
- [x] `agent_ui_resolved` between the requests and before the UI list response is emitted after the Snapshot (`a_push_that_raced_the_list…`)
- [x] UI list timeout / error / `ok:false` → empty list, live, warning; `list_agents` failure → backoff (`ui_list_failures_…`, `list_agents_failure_backs_off`)
- [x] 1 MB message + 500 options → ≤2048 B, ≤100 options, `truncated` (`preview_caps_strings_on_char_boundaries_and_lists`, multi-byte `é`)
- [x] `expectsResponse:false` / `removed:true` not in `uiPending` (`removed_and_non_responding_entries_are_not_attention`, snapshot test)
- Extra: reconnect re-syncs with epoch + 1; push buffer overflow rules (unit tests).

## Smoke against pi-studio
Dev daemon 127.0.0.1:6790 (mock). Agents created with the CLI: `hello` (idle), `#ui confirm`, `#ui select`, `#stream fail`. `cargo test -p pistudiodashboard-client --test live_daemon live_daemon_snapshot_and_mutations -- --ignored --nocapture` (env: `PI_STUDIO_SMOKE_CONFIRM_AGENT`, `_INTERRUPT_AGENT`, `_IDLE_AGENT`):
```
Probing · Health(true) · Connecting · Handshaking · Hello · Syncing · Live (seq 7) · Snapshot (seq 8)
SNAPSHOT: 11 agents, 5 pending dialogs
  agent f469dce4… error "#stream fail"     agent a9b02ffb… error "#stream fail"     (the failing agents are `error`)
  agent 8e943c53… running "#ui confirm"    agent f593b936… running "#ui select"     agent ca320c4e… idle "hello"
  pending ec8a3cf9… agent=8e943c53… method=confirm expects=true title="Clear session?" options=None
  pending 9a805ef5… agent=f593b936… method=select  expects=true title="Allow this extension to modify /etc/hosts?" options=["Allow","Block"]
  (+3 more confirm dialogs from earlier `#ui confirm` agents)
```
So the real snapshot carries `ui_pending` for `#ui confirm` / `#ui select` agents; both stay `running`. Mutation half: see task-004 (`Ok`, then `NotFound`, `Interrupt{ok:true}`, `UiResolved{answered}` pushed live and dispatched as an event). First `Diagnostics` after sync: `messagesTotal:2`.

## Follow-ups
- Reducer arms for `UiResolved` / `RequestResult` (sprint-007). Typed push events for agent updates/streams (sprint-006).
- `Diagnostics` carries no `lastError`; the ui-list warning lives on the `Live` `ConnState`. The UI should treat a `Live` state with `lastError` as a warning, not a failure.
