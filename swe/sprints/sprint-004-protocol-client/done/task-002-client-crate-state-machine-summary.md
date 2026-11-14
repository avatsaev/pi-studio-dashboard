# Task 002 — Client crate and connection state machine — Summary

- **Sprint:** sprints/sprint-004-protocol-client
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
New L4 crate `pistudiodashboard-client`: `ClientHandle::{start, try_start, start_in, connect, disconnect, remove, restart, shutdown}`, one tokio task per `entryId` running the state machine (`probing → connecting → handshaking → syncing → live`, `backoff`, terminal `auth_failed`/`host_rejected`, user `idle`), a hand-written HTTP/1.0 health probe (3 s, no credentials), the pure `backoff::delay(attempt, unit)` schedule, per-entry batching (50 ms / 256 events, bounded queues, gate for `remove`), rustls TLS with native roots, bearer-subprotocol offering and a redacted `Secret`.

Events reuse `pistudiodashboard_model::{DaemonEvent, DaemonEventKind, ConnState, ConnErrorView, ServerInfoView, ServerCaps}` (sprint-002/task-001); no second definition. `ConnError{kind,message}` maps to `ConnErrorView{code,message}` with codes `unreachable, auth_failed, host_rejected, bad_handshake, frame_too_large, closed, timeout`.

## Files created / changed
| File | Change |
|------|--------|
| `crates/client/Cargo.toml`, `src/{lib,handle,conn,emitter,batch,backoff,probe,tls,state,secret}.rs` | created |
| `crates/client/tests/{connection,live_daemon}.rs`, `tests/support/{mod,fake_daemon}.rs` | created (fake daemon: HTTP health + WS, scriptable upgrade status / handshake / close / oversize / silence) |
| `Cargo.toml` (workspace) | +7 external `[workspace.dependencies]` lines: tokio, tokio-tungstenite, futures-util, rustls, tokio-rustls, rustls-native-certs |

## How it satisfies the scope
Per protocol-client.md § Connection state machine / Events / Backpressure / WebSocket-TLS. Decisions and deviations:
- Health probe is strict (`200` + `{status:"ok"}`); one failed probe goes to backoff, the next cycle attempts the socket even if the probe fails again (`failed_probe_gates_the_socket_once_per_cycle`).
- 401/403 → terminal states and the `pi-studio.bearer.<pw>` subprotocol are implemented here (small, and the state machine is incomplete without them); task-003 still owns ping/pong, counters, fixtures for its own criteria. Password never appears in events/logs (tested).
- `ClientHandle::start_in(Handle, cfg)` added so tests run on a paused-clock runtime; `start` owns a 2-worker runtime.
- `connEpoch` counters live per entry id in the handle and survive remove/re-add and restart.
- Oversized frames: tungstenite `Capacity` error with 16 MiB limits → `frame_too_large`. `framesTooLarge` counter belongs to task-003.
- Live loop currently decodes and drops frames (typed handling/counters in later tasks); no ping timer yet.
- Licences (`cargo metadata`): tokio 1.54.1 MIT; tokio-tungstenite 0.30.0 MIT; tungstenite 0.30.0 MIT OR Apache-2.0; futures-util 0.3.34 MIT OR Apache-2.0; rustls 0.23.46 Apache-2.0 OR ISC OR MIT; tokio-rustls 0.26.6 MIT OR Apache-2.0; rustls-native-certs 0.8.5 Apache-2.0 OR ISC OR MIT; rustls-webpki 0.103.17 ISC; ring 0.17.14 Apache-2.0 AND ISC. Roots come from the system store (no MPL `webpki-roots`); `rustls` uses the `ring` provider, so no aws-lc and no OpenSSL linkage (`openssl-probe` only finds cert dirs). No deny list exists; `ATTRIBUTION.md` is for assets only, so not edited. Self-signed `wss://` certs are rejected (TODO(verify) stays open).

## Build & test results
```
$ cargo test -p pistudiodashboard-client
11 unit passed; 14 integration passed (tests/connection.rs); 2 ignored (live_daemon); 0 failed
$ cargo clippy -p pistudiodashboard-client --all-targets -- -D warnings   # clean
$ cargo fmt -p pistudiodashboard-client --check                           # clean
$ cargo xtask layers   -> layering: ok (11 crates)
```

## Acceptance criteria
- [x] Event order probing, Health(true), connecting, handshaking, Hello, syncing, live; one epoch, increasing seq; first frame `hello` clientType cli (happy_path_…)
- [x] Close → backoff with nextRetryInMs, next attempt epoch+1, seq restarts (daemon_close_then_backoff_then_next_epoch)
- [x] Delays ±20% of 0.5,1,2,4,8,16,30,30 s on simulated clock; reset to ≈0.5 s after ≥10 s live (unreachable_port_…, attempt_counter_resets_…, backoff unit tests)
- [x] Disconnect: close 1000, idle, silent afterwards; remove: nothing after return (disconnect_…, remove_…)
- [x] 1,000 events → ordered batches ≤256; full channel stalls producer (batch.rs unit tests)
- [x] Bad first frame / missing or empty serverId / non-JSON → backoff `bad_handshake` (bad_first_frame_…); silent daemon → `timeout`
- [x] connect twice → one attempt (connect_twice_is_one_attempt)
- [x] Layers: client is L4, nothing depends on it (`cargo xtask layers` ok). `cargo xtask wasm` not run (xtask WASM_EXCLUDED already lists client).

## Smoke against pi-studio
Dev daemon 127.0.0.1:6790 (mock). `PI_STUDIO_SMOKE=127.0.0.1:6790 cargo test -p pistudiodashboard-client --test live_daemon -- --ignored --nocapture --test-threads=1` (owned multi-thread runtime) → 2 passed. Live daemon, abridged:
```
epoch=1 seq=1 Probing   | seq=2 Health{reachable:true} | seq=3 Connecting | seq=4 Handshaking
seq=5 server=693facb5-3eb8-46a2-b6c6-8a4a00a850b2 Hello{ hostname:"127.0.0.1", version:"0.0.115", features:[agentTodos,…,daemonObservability,daemonStatusRpc,…15], caps{daemon_observability:true}, degraded:{}, tls_in_use:false, is_loopback:true }
seq=6 Syncing | seq=7 Live   ... client.disconnect → seq=8 Idle (daemon saw close, no further events)
```
Closed port (reconnect/backoff, real clock): Probing → Health{false} → Backoff(417 ms, "health probe failed") → epoch 2 Probing/Health(false)/Connecting/Backoff(929 ms, "Connection refused") → epoch 3 (1705 ms) → epoch 4 (3887 ms): doubling with jitter, epoch+1, seq restarting at 1. RTT is not emitted yet (ping/pong is task-003). The daemon was not killed or restarted.

## Follow-ups / TODO(verify)
- `Secret::expose` is crate-private; the app binary constructs `Secret::new`.
- Self-signed wss and unsupported `protocolVersion` behaviour remain unverified. TLS path is only compile-checked (no test against a real TLS server).
- 2-missed-pongs liveness, counters, requests, sync: tasks 003–005.
