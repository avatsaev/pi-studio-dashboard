# Task 002 — Client crate and connection state machine

- **Sprint:** sprints/sprint-004-protocol-client
- **Status:** done
- **Type:** feature
- **Area:** crates/client (L4)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-001, sprint-002-domain-cutover/task-001

## Goal
Create `pistudiodashboard-client` (Layer 4, not wasm-checked) with `ClientHandle`, one connection task per entry, the connection state machine, backoff, health probe, and batched `DaemonEvent` emission, tested against a scripted fake daemon.

## Context / why
This is the only code that opens sockets (D3). It owns a 2-worker tokio runtime and turns each daemon into an ordered stream of `DaemonEvent`s tagged with `connEpoch` (D38) and per-connection `seq`. Auth/handshake details, request correlation, sync and stats are layered on in tasks 003–005 and sprint-006; this task delivers the skeleton that connects, probes, handshakes with `hello` (no auth yet), reconnects with backoff, and emits `ConnState`/`Hello`/`Health` events.

## Scope references
- swe/specs/architecture/protocol-client.md § Connection state machine
- swe/specs/architecture/protocol-client.md § Events emitted by L4
- swe/specs/architecture/protocol-client.md § Backpressure on the dashboard side
- swe/specs/architecture/protocol-client.md § WebSocket / TLS crate candidates
- swe/specs/architecture/app-structure.md § Layer / crate map, § Threads
- `crates/client/` (new: `Cargo.toml`, `src/lib.rs`, `handle.rs`, `conn.rs`, `state.rs`, `backoff.rs`, `probe.rs`, `batch.rs`, `tests/support/fake_daemon.rs`)
- `xtask/src/layers.rs` (`TABLE`: `("client", Class::Layer(4))`)
- `xtask/src/main.rs` (`cmd_wasm` list must exclude `client`)
- `crates/testkit/` (scripted fake daemon helpers may live in `crates/client/tests/support/` and be re-exported only if reused)

## What to build
Crate `crates/client`, package `pistudiodashboard-client`, `#![forbid(unsafe_code)]` with the workspace's non-test lint set (no `unwrap`/`expect`/`panic` outside tests). Register in `xtask/src/layers.rs` `TABLE` at `Class::Layer(4)`; confirm `cargo xtask wasm` does not include it (sprint-001/task-005 made the wasm list explicit — add nothing to it). Layers: it may use protocol, model, ingest (event types), config (entry types).

Public contract:
- `ClientHandle::start(config: ClientConfig) -> (ClientHandle, mpsc::Receiver<Vec<DaemonEvent>>)`; the runtime is a multi-thread tokio runtime with 2 workers owned by the handle (dropping/`shutdown()` stops all tasks). `ClientConfig { app_version, event_channel_capacity: 1024 batches (test-overridable) }`.
- Per-entry control: `connect(entry: DaemonEntry, password: Option<Secret>)`, `disconnect(entry_id)`, `remove(entry_id)`, `restart(entry_id, entry, password)` (edit / setPassword). One task per `entryId`; `connect` while connecting/live is idempotent.
- `DaemonEvent { entry_id, server_id: Option<String>, conn_epoch: u64, seq: u64, at: epoch_ms, kind }` and `kind` variants `ConnState{state, since, last_error?, next_retry_in_ms?}`, `Hello{info, caps, degraded, tls_in_use, is_loopback}`, `Health{reachable, at}` (others are added by later tasks). `ConnState` states and error kinds as in the state machine; `ConnError{kind, message}` kinds: `unreachable`, `auth_failed`, `host_rejected`, `bad_handshake`, `frame_too_large`, `closed`, `timeout`. `DaemonEvent` must be defined where `model`/`ingest` can consume it without depending on `client` (per app-structure: in `model` or `protocol` — follow whatever sprint-002/task-001 chose; do not invent a second definition).
- `conn_epoch` is a per-entry counter incremented when each connection attempt starts; the first event of an attempt is its `ConnState{probing}`. `seq` is monotonic within one connection attempt (resets with a new epoch).
- State machine exactly per spec: `idle → probing → connecting → handshaking → syncing → live`, any → `backoff → probing`; terminals `auth_failed`, `host_rejected`, user `idle`. In this task `syncing` completes immediately after `Hello` (task-005 fills it) so `live` is reachable. Timeouts: probe 3 s, TCP+upgrade 5 s, `server_info` wait 5 s. Probe = unauthenticated `GET http(s)://host:port/api/health`; one failed probe emits `Health{reachable:false}` and goes to backoff (and per spec starts the WS attempt anyway after one failed probe per backoff cycle — implement as documented, see Notes).
- Backoff: `delay = min(30 s, 0.5 s · 2^attempt) · (1 ± 0.2 uniform jitter)`; attempt resets to 0 once the connection was `live` ≥ 10 s. Implement the schedule as a pure function taking an injected RNG/jitter source so it is testable without sleeping; the task uses `tokio::time::pause()`-style simulated time in tests.
- Batching: events accumulate per entry and flush as one `Vec<DaemonEvent>` on the bounded channel when 50 ms elapse or 256 events accumulate, whichever first; per-entry order is preserved.
- WebSocket: `tokio-tungstenite` (MIT) unless the licence/feature check below says otherwise; `max_message_size`/`max_frame_size` set to 16 MiB in task-003 wiring (set the config here); the rejected-upgrade HTTP status must be reachable (tungstenite `Error::Http(Response)`) — wrap so 401/403 survive. TLS: `rustls` via `tokio-rustls`/the tungstenite `rustls-tls-*` feature, roots from `rustls-native-certs` or `webpki-roots`; no OpenSSL. Self-signed certs are rejected (no override in v1). `tls_in_use`/`is_loopback` computed from the entry (loopback = `127.0.0.0/8`, `::1`, `localhost`).
- Fake daemon (`crates/client/tests/support/`): in-process tokio WebSocket server + tiny HTTP `/api/health` responder; scriptable: accept/refuse upgrade with a given status, send `status/server_info` from the vendored fixture (serverId overridable), close/drop the connection, delay or stay silent. Reused by tasks 003–005 and sprint-006 tests.
- Logging (if any): connection transitions only, URL without credentials; never passwords/subprotocols.

## Out of scope
- Bearer-subprotocol auth, 401/403 mapping, ping/pong, diagnostics counters (task-003).
- `OutboundRequest` and request correlation (task-004).
- `list_agents`/`agent_ui_list` sync and `Snapshot` (task-005); stats polling (sprint-006/task-003); timeline fetch (sprint-006/task-005).
- Wiring into the app binary / `feed.ingest` (sprint-005/task-001).
- Stage-2 subscriptions (sprint-010).

## Acceptance criteria
- [ ] Given a fake daemon, when `connect` is called, then events arrive in order `ConnState{probing}`, `Health{reachable:true}`, `ConnState{connecting}`, `ConnState{handshaking}`, `Hello`, `ConnState{syncing}`, `ConnState{live}`, all with the same `connEpoch`, strictly increasing `seq`, and the first frame the daemon received is `hello` with `clientType:"cli"`.
- [ ] Given the daemon closes the socket, when the client reconnects, then the new attempt's events carry `connEpoch` + 1 and `seq` restarts, and `ConnState{backoff}` precedes it with `nextRetryInMs` set.
- [ ] Given an unreachable port, when retrying over simulated time, then successive delays are within ±20% of 0.5, 1, 2, 4, 8, 16, 30, 30 s (cap), and after a connection that stayed `live` ≥ 10 s the next delay is back at ≈ 0.5 s. (D25)
- [ ] Given `disconnect`, then the socket closes with code 1000, state is `idle`, no timer fires afterwards; given `remove`, then no event for that entry is emitted after the call returns.
- [ ] Given 1,000 events produced in < 50 ms, then they leave as batches of ≤ 256 events and order is preserved; given a channel that is full, then the task applies backpressure to the reader (no unbounded queue).
- [ ] Given a first frame that is not `status/server_info`, or a `server_info` without `serverId`, then state goes to `backoff` with `ConnError{bad_handshake}`.
- [ ] Given `connect` twice while connecting, then only one attempt runs (idempotent).
- [ ] Given `cargo xtask layers`, then `client` is Layer 4 and nothing in L0–L3/engine/shell depends on it; given `cargo xtask wasm`, then `client` is not built for wasm32.

## Test / verification plan
- `cargo test -p pistudiodashboard-client` — tests in `crates/client/tests/` using the fake daemon; backoff schedule test as a unit test over the pure function plus one simulated-clock integration test.
- `cargo xtask layers` and `cargo xtask wasm` pass.
- Licence check (golden rule 9): for every crate added (`tokio`, `tokio-tungstenite` + its TLS feature, `tokio-rustls`/`rustls`, `rustls-native-certs` or `webpki-roots` (MPL-2.0 data — decide and record), `url`, an HTTP probe client such as `hyper`/`hyper-util`/`reqwest`), read the licence from the crates.io page / `cargo metadata`, confirm permissive (MIT/Apache-2.0/ISC/BSD; MPL only if the data-only exception is justified), and record crate, version, licence in the task summary and `ATTRIBUTION.md`/deny list if one exists.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): Licences of tokio-tungstenite/fastwebsockets, rustls, rustls-native-certs/webpki-roots, reqwest/hyper — resolves the brief's licence TODO for the WebSocket crate.
- TODO(verify) (spec): a health probe failure never blocks the WS attempt more than once per backoff cycle — acceptable? Implement as written: after one failed probe, the next cycle attempts the WS connection even if the probe fails again.
- TODO(verify) (spec): whether self-signed `wss://` certs need a user override (outside the brief); v1 rejects them.
- Pick a health-probe client with a tiny dependency surface; it must apply the 3 s timeout and send no credentials/`Authorization`.
- Planning note: the outline lists dependency `S002/T001` (ingest crate) because `DaemonEvent`/`Effect` types are defined for `ingest` to consume; reuse that definition.
