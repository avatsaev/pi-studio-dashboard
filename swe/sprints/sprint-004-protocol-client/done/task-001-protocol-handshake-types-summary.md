# Task 001 — Protocol handshake types — Summary

- **Sprint:** sprints/sprint-004-protocol-client
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
`pistudiodashboard-protocol` gained `handshake.rs` (`Hello`, `ClientType::Cli`, `ServerInfo`, `HandshakeError::BadHandshake`, `ServerCaps`, `DegradedReason`), `liveness.rs` (`Ping` encoder, `Pong` decoder), `error.rs` (`RpcError`), `health.rs` (`HealthBody`) and `inbound.rs` (`decode_inbound(text) -> Result<Inbound, FrameError>`; `Inbound` = `ServerInfo | BadHandshake | Status | Pong | RpcError | Session | Malformed{type_} | Unknown{type_}`). `decode_frame`/`Frame` from sprint 001 are unchanged and `decode_inbound` is built on them.

## Files created / changed
| File | Change |
|------|--------|
| `crates/protocol/src/{handshake,liveness,error,health,inbound}.rs` | created |
| `crates/protocol/src/lib.rs` | modules + re-exports |
| `crates/protocol/tests/handshake.rs` | created (13 tests, fixture-driven) |

## How it satisfies the scope
Wire shapes follow protocol-client.md § Handshake messages and the vendored `hello/status.server_info/ping/pong/rpc_error/health` fixtures plus `local/status.server_info.no_observability.json`. Strings reaching the UI are capped (256 B server_info, 2048 B rpc_error text). Deviations: (1) tests read fixtures from disk with std/serde_json, not the testkit loader, because `protocol` is `Class::Standalone` (no workspace dev-deps; testkit itself depends on protocol). (2) `hello.json` fixture capabilities (`reasoning_merge_enum`) are ignored in comparison; we send `capabilities:{}` per spec — live daemon accepted it.

## Build & test results
```
$ cargo test -p pistudiodashboard-protocol   # 11 unit + 13 integration + 1 doctest passed, 0 failed
$ cargo clippy -p pistudiodashboard-protocol --all-targets -- -D warnings   # clean
$ cargo fmt -p pistudiodashboard-protocol    # applied
$ cargo check -p pistudiodashboard-protocol --target wasm32-unknown-unknown # ok
```

## Acceptance criteria
- [x] Hello encode vs fixture (hello_encodes_like_fixture)
- [x] server_info fields/maps (server_info_fixture, server_info_is_lenient)
- [x] missing/empty serverId → BadHandshake (bad_server_info_is_typed_error)
- [x] caps/degraded table incl. daemonStatusRpc-only and `"yes"` (degraded_table, local_no_observability_fixture_is_degraded)
- [x] pong fixture + ISO variant + missing requestId → Malformed (pong_*)
- [x] rpc_error fixture + unknown field (rpc_error_fixture_and_tolerance)
- [x] health fixture + extra fields, is_ok only for "ok" (health_fixture_and_variants)

## Smoke against pi-studio
Dev daemon 127.0.0.1:6790 (mock). Throwaway Rust example printed `Hello::new("smoke-1","0.1.0").encode()`, a `Ping`, and a session message of unknown type; a node script sent them over `ws://127.0.0.1:6790/ws` and recorded replies; the example piped them through `decode_inbound` (all deleted afterwards):
```
ServerInfo { id:693facb5-… host:Some("127.0.0.1") ver:Some("0.0.115") home:Some("/home/avatsaev") caps:{} obs:true degraded:{} features:15 }
Pong(Pong { request_id: "p1", server_received_at: 1794610309637, server_sent_at: 1794610309637, client_sent_at: Some(1767225600000) })
RpcError(RpcError { request_id: "e1", code: Some("unknown_message_type"), message: Some("no handler for no_such_type_zzz"), error: None })
$ curl -s 127.0.0.1:6790/api/health   ->  {"status":"ok"}
```
The daemon accepted our `hello` (empty capabilities, `clientType:"cli"`), replied with `status/server_info` (no server hello), and this live daemon advertises `daemonObservability:true`. Health body is the fixture shape (decoded in the fixture test; not fed through Rust live).

## Follow-ups / TODO(verify)
- Spec check: matches. `Inbound::Malformed` and `BadHandshake` are for L4 (task-002/003) to count/close.
- Daemon behaviour for unsupported `protocolVersion` still unverified.
