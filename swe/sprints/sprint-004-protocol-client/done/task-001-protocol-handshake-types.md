# Task 001 — Protocol handshake types

- **Sprint:** sprints/sprint-004-protocol-client
- **Status:** done
- **Type:** feature
- **Area:** crates/protocol (L0)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** sprint-001-core-crates/task-002

## Goal
Add typed handshake, liveness, error and health wire types to `pistudiodashboard-protocol`, plus the `ServerCaps`/`DegradedReason` derivation.

## Context / why
Everything the client needs before any agent traffic: the `hello` it sends, the `status/server_info` it receives, `ping`/`pong`, `rpc_error`, the `/api/health` body. Capability gating (D26/D30) is decided here once so L4 and the reducers share it.

## Scope references
- swe/specs/architecture/protocol-client.md § Handshake messages
- swe/specs/architecture/protocol-client.md § Feature flags → capability set (D26)
- swe/specs/architecture/protocol-client.md § Lenient decoding rules
- swe/specs/architecture/protocol-client.md § Ping/pong RTT
- `crates/protocol/src/` (add `handshake.rs`, `liveness.rs`, `error.rs`, `health.rs`; extend `lib.rs`/the envelope module from sprint-001/task-001)
- crates/protocol/tests/fixtures/pi-studio/{hello,status.server_info,ping,pong,rpc_error,health}.json (vendored in sprint-001/task-002)

## What to build
Create/modify in `crates/protocol/src/`. Contracts (exact wire shapes from the fixtures):

- `Hello` (C→S, top-level, not enveloped): `{type:"hello", clientId:String, clientType:"cli", protocolVersion:1, appVersion:"pistudiodashboard/<semver>", capabilities:{}}`. Encoder-only constructor `Hello::new(client_id, app_version)`; `clientType` is a closed enum with the single variant `Cli` (D23). `appVersion` is built from the crate/app version passed in, prefixed `pistudiodashboard/`.
- `ServerInfo` from `{type:"status", payload:{status:"server_info", serverId, hostname?, version?, homeDir?, capabilities:{}, features:{…}}}`: `serverId` required and non-empty (missing/empty → typed `BadHandshake` error, not a panic); `hostname/version/homeDir` optional (lenient: wrong type → `None`); `features` and `capabilities` are `BTreeMap<String, serde_json::Value>`; only literal `true` counts.
- `ServerCaps { daemon_observability: bool }` derived by `ServerCaps::from_features(&features)`: `true` iff `features.daemonObservability == true`. The legacy `daemonStatusRpc` flag is ignored (D30). `DegradedReason` = `{NoObservability}` (wire/serde name `no_observability`); `ServerCaps::degraded() -> BTreeSet<DegradedReason>`.
- `Ping {requestId, clientSentAt: epoch_ms}` (encoder, top-level) and `Pong {requestId, serverReceivedAt, serverSentAt, clientSentAt?}` (decoder): timestamps use the shared `number | ISO string → epoch ms` field deserialiser from sprint-001/task-001.
- `RpcError {requestId, code?, message?, error?}` decoded from a session message of type `rpc_error` (`code` open string: `unknown_message_type`, `handler_error`, …).
- `HealthBody {status}` for `GET /api/health` (`{status:"ok"}`); decoder tolerant of extra fields; `is_ok()`.

Wire the new top-level/session variants into the inbound decode entry point so a `status`, `pong` frame and a session `rpc_error` message decode to typed values; everything else keeps decoding as `Unknown{type}`.

## Out of scope
- Outbound request enum, `OutboundRequest`, allowlist (task-004).
- Sync/list types (task-005), agent/stream/dialog types (sprint-006, sprint-007), stage-2 types (sprint-010).
- Any I/O or the client crate (task-002).
- Diagnostics counters for unknown types (counted in L4, task-003).

## Acceptance criteria
- [ ] Given `hello.json`, when `Hello::new` is encoded, then the JSON has `type:"hello"`, `clientType:"cli"`, `protocolVersion:1`, `appVersion` starting `pistudiodashboard/`, and a `clientId`; the decoded fixture and the encoded value agree on every field except the volatile `/clientId` and `capabilities` (see Notes).
- [ ] Given `status.server_info.json`, when decoded, then `serverId`, `hostname`, `version`, `homeDir` are exposed and `features`/`capabilities` are maps.
- [ ] Given a `server_info` with missing or empty `serverId`, when decoded, then the result is the typed bad-handshake error, never a panic.
- [ ] Given `features:{daemonObservability:true}` then `daemon_observability` is true and degraded is empty; given only `daemonStatusRpc:true`, or `daemonObservability:"yes"`, or no flag, then `daemon_observability` is false and degraded = {no_observability}. (D26, D30)
- [ ] Given `pong.json` and a variant with ISO-string timestamps, then both decode to epoch-ms numbers; given a pong missing `requestId`, then it is `malformed`.
- [ ] Given `rpc_error.json`, then `requestId`, `code`, `message` are exposed; given an extra unknown field, then it is tolerated.
- [ ] Given `health.json` body and a body with extra fields, then both decode; `is_ok()` is true only for `status:"ok"`.

## Test / verification plan
- `cargo test -p pistudiodashboard-protocol` — add `tests/handshake.rs` that loads the vendored fixtures via the testkit loader from sprint-001/task-002 and asserts the table above; one test of `degraded` inputs in `tests/local` fixtures (`server_info` without `daemonObservability`, `daemonObservability` of a non-bool type).
- `cargo xtask wasm` passes (types are wasm-safe, no I/O).
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): daemon behaviour on an unsupported `protocolVersion` or invalid `clientType` — `clientType` is a strict enum `mobile|browser|cli|mcp` in pi-studio `helloSchema`, we always send `cli`.
- PLANNING FLAG: `hello.json` fixture carries `capabilities:{reasoning_merge_enum:true}` while the spec says the dashboard advertises `capabilities:{}`. Follow the spec (advertise nothing) and make the fixture comparison ignore `capabilities`; if the daemon turns out to require a capability, revisit.
- `status.server_info.json` fixture has no `daemonObservability` (stage 2 not yet in the fixture's feature map); that is the correct degraded input for tests.
- Strings that reach the UI are capped at decode (hostname/version/homeDir ≤ 256 chars, rule 6 of lenient decoding).
