# Task 001 — Protocol crate: envelope, frame cap, lenient decoding

- **Sprint:** sprints/sprint-001-core-crates
- **Status:** done
- **Type:** feature
- **Area:** L0 `crates/protocol` (wire frames)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** none

## Goal
Create the L0 crate `pistudiodashboard-protocol` that decodes and encodes the top-level pi-studio frames (`hello`, `status`, `ping`, `pong`, `session{message}`) with a 16 MiB cap, typed errors and the lenient-decoding helpers every later wire type will reuse.

## Context / why
Every other crate (model, ingest, client, engine) consumes pi-studio's wire protocol. The daemon has no role gating and sends whatever a newer version sends, so decoding must be lenient (D22, D26) and hostile-input safe (rule 2, D28). This task builds only the envelope layer and the shared lenient primitives; typed messages arrive in sprints 004, 006, 007 and 010. Nothing user-visible changes; the sample checklist app keeps working beside the new crate.

## Scope references
- `swe/specs/architecture/protocol-client.md` § Envelope and frame rules, § Lenient decoding rules, § Messages the dashboard consumes (the `pong` row: timestamp normalisation)
- `swe/specs/architecture/app-structure.md` § Layer / crate map (L0 row), § Wasm check scope
- `swe/grills/daemon-monitoring-dashboard.md` D22, D26, D28
- `xtask/src/layers.rs` (`TABLE`; `Class::Standalone`), `Cargo.toml` (workspace members use `crates/*`, `[workspace.dependencies]` internal list)
- Create `crates/protocol/{Cargo.toml,src/lib.rs,src/frame.rs,src/lenient.rs}`

## What to build
- New crate, package `pistudiodashboard-protocol`, `#![forbid(unsafe_code)]` + the same `deny(clippy::unwrap_used, …)` header as `crates/engine/src/lib.rs`; workspace lints; **no workspace dependencies** (`Class::Standalone` rejects them, including dev-deps). Dependencies: `serde`, `serde_json`, `thiserror` (all already in `[workspace.dependencies]`; no new third-party crate, so no licence check needed). Add `pistudiodashboard-protocol` to `[workspace.dependencies]` and `("protocol", Class::Standalone)` to `TABLE`.
- `frame.rs` — public contract:
  - `pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;`
  - `pub enum Frame { Hello(serde_json::Value), Status { payload: Value }, Ping { request_id: Option<String>, client_sent_at: Option<i64> }, Pong { request_id: Option<String>, client_sent_at: Option<i64>, server_received_at: Option<i64>, server_sent_at: Option<i64> }, Session { message: Value }, Unknown { type_: String } }` — payloads are kept as `Value` here on purpose: typed `hello`/`server_info` land in sprint-004/task-001, session messages in later sprints. `Unknown` is the fallback for any other top-level `type` (never an error).
  - `pub fn decode_frame(text: &str) -> Result<Frame, FrameError>` and `pub fn encode_frame(&Frame) -> String` (round-trip support for tests; the outbound allowlist encoder comes in sprint-004/task-004).
  - `pub enum FrameError { TooLarge { bytes: usize }, NotJson, NotObject, MissingType }` with `Display` messages. Size is checked **before** parsing. `TooLarge` at `len > MAX_FRAME_BYTES` (exactly 16 MiB is accepted).
- `lenient.rs` — reusable primitives for later wire types (each with unit tests):
  - `WireTime` / `fn wire_time_ms(&Value) -> Option<i64>`: `number | ISO-8601 string` → epoch ms; unparsable string → `None`. Implement ISO parsing by hand (no new crate) for `YYYY-MM-DDTHH:MM:SS[.fff](Z|±hh:mm)`.
  - A serde `deserialize_with` helper `lenient_opt` that decodes a wrong-typed optional field as `None` instead of failing the message.
  - `fn cap_str(s: &str, max_bytes: usize) -> (String, bool)` truncating on a UTF-8 boundary and reporting truncation.
  - An `Unknown`-fallback convention for open enums (a documented serde pattern with a unit test on a sample enum); the first real wire enums (`AgentStatus`, stream-event kinds) are defined in later tasks using it.
- Frames carry no secrets in `Debug` output beyond what the caller puts in; do not log frame bodies anywhere.

## Out of scope
- Typed `hello`, `status/server_info`, `rpc_error`, health body, `ServerCaps` → sprint-004/task-001.
- `OutboundRequest`, `MUTATION_ALLOWLIST` and the outbound encoder → sprint-004/task-004.
- `list_agents_response`/`agent_ui_list_response` → sprint-004/task-005; agent/stream/stats/timeline types → sprint-006/task-001; dialog types → sprint-007/task-001; observability types → sprint-010/task-001.
- Binary-frame handling and the `malformed`/`framesTooLarge` counters (L4, sprint-004/task-002+).
- Vendored fixtures (task-002).

## Acceptance criteria
- [ ] Given `{"type":"ping"}`, `{"type":"pong","requestId":"r","serverReceivedAt":1,"serverSentAt":2}`, `{"type":"status","payload":{…}}`, `{"type":"session","message":{"type":"x"}}`, when decoded, then the matching `Frame` variant is returned and `encode_frame` → `decode_frame` round-trips each.
- [ ] Given a frame with extra unknown fields at every level, when decoded, then it succeeds (no `deny_unknown_fields`).
- [ ] Given `{"type":"terminals_update"}` (any unknown top-level type), then `Frame::Unknown` carries the type string and no error occurs.
- [ ] Given invalid JSON, a JSON array/number, and an object without a string `type`, then the errors are `NotJson`, `NotObject`, `MissingType` respectively and nothing panics.
- [ ] Given a text of `MAX_FRAME_BYTES + 1` bytes, then `FrameError::TooLarge` is returned without allocating a parse tree; given exactly `MAX_FRAME_BYTES` of valid JSON, then it decodes.
- [ ] Given timestamps `1700000000000`, `"2026-11-13T22:13:10Z"`, `"2026-11-13T22:13:10.250+01:00"` and `"garbage"`, then `wire_time_ms` yields the epoch-ms values and `None`.
- [ ] Given an optional field of the wrong JSON type, when decoded with `lenient_opt`, then the field is `None` and sibling fields survive.
- [ ] Given `cap_str` on a multi-byte string cut mid-codepoint, then the result is valid UTF-8 and the flag is `true`.
- [ ] `cargo xtask wasm` builds `pistudiodashboard-protocol` for `wasm32-unknown-unknown` (it is picked up from `TABLE`).

## Test / verification plan
- Build: `cargo build -p pistudiodashboard-protocol`.
- Tests: unit tests in `crates/protocol/src/{frame,lenient}.rs` covering the criteria above (a table test for `wire_time_ms`; a hostile-input test for the three decode errors; the size-cap boundary test). Add one `proptest`-free fuzz-ish test: decode of 1,000 pseudo-random byte strings (as lossy UTF-8) never panics.
- Focused: `cargo test -p pistudiodashboard-protocol`; `cargo xtask layers` (crate registered, no workspace deps).
- Gate: `cargo xtask ci` passes.

## Notes
- Timestamp handling per protocol-client rule 4: all epoch-ms values are `i64`.
- TODO(verify) (protocol-client.md): daemon behaviour for an unsupported `protocolVersion` — not needed here.
- Keep the module layout flat so later wire-type modules (`handshake.rs`, `outbound.rs`, `agent.rs`, …) slot in next to `frame.rs`.
