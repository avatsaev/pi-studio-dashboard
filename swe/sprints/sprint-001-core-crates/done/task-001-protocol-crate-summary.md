# Task 001 — Protocol crate: envelope, frame cap, lenient decoding — Summary

- **Sprint:** sprints/sprint-001-core-crates
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
New L0 crate `pistudiodashboard-protocol` (no workspace deps): `Frame`/`FrameError`/`decode_frame`/`encode_frame` with a 16 MiB cap checked before parsing, and `lenient` helpers (`wire_time_ms`, `lenient_opt`, `cap_str`, documented `#[serde(other)]` open-enum pattern).

## Files created / changed
| File | Change |
|------|--------|
| `crates/protocol/Cargo.toml` | created |
| `crates/protocol/src/{lib,frame,lenient}.rs` | created, with unit tests |

## How it satisfies the scope
Per protocol-client.md envelope/lenient rules. Deviation: the `TABLE` registration in `xtask/src/layers.rs` is done by task-005. `Hello` keeps the whole frame object.

## Build & test results
```
$ cargo test -p pistudiodashboard-protocol      # 11 passed, 0 failed
$ cargo clippy -p pistudiodashboard-protocol --all-targets -- -D warnings   # clean
$ cargo fmt -p pistudiodashboard-protocol --check   # clean
$ cargo check -p pistudiodashboard-protocol --target wasm32-unknown-unknown # ok
```

## Acceptance criteria
- [x] Frame variants + round trip (known_frames_round_trip)
- [x] Unknown fields tolerated (unknown_fields_tolerated)
- [x] Unknown type fallback (unknown_type_is_fallback)
- [x] NotJson/NotObject/MissingType (hostile_inputs)
- [x] Size cap boundary (size_cap_boundary)
- [x] wire_time_ms table (wire_time_table)
- [x] lenient_opt (lenient_opt_wrong_type_is_none)
- [x] cap_str (cap_str_boundaries)
- [ ] `cargo xtask wasm` pickup depends on task-005 TABLE entry; direct wasm32 check passes.

## Follow-ups / TODO(verify)
- `cargo xtask layers`/`wasm` once task-005 registers the crate.

## Smoke against pi-studio
Daemon at 127.0.0.1:6790 (already running, mock provider). Created an agent with `$PS run "hello" ...` (`$PS ls` showed 2 agents). A throwaway node script (global `WebSocket`, deleted afterwards) sent `hello{clientId,clientType:"browser",protocolVersion:1}` then `ping{requestId,clientSentAt}` / `session{message:{type:"list_agents_request"}}`, and wrote each received text frame as a line; a throwaway `examples/stdin_decode.rs` (deleted) piped them through `decode_frame`:
```
Status { payload: Object {"capabilities": Object {}, "features": Object {"agentTodos": Bool(true), ...
Pong { request_id: Some("p1"), client_sent_at: Some(1794610059291), server_received_at: Some(1794610059292), s...
Session { message: Object {"agents": Array [Object {"agentId": String("157fe5cb-...
```
Observed: the daemon sends no `hello` frame; it replies to the client's `hello` with `status` (server_info). A non-`hello` first client frame closes the socket with 1008 (my first attempt). `list_agents_response` arrives wrapped in a `session` frame. All three decoded with no errors; no dependency added.
