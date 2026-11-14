# Task 001 — App runtime wiring — Summary

- **Sprint:** sprints/sprint-005-daemon-connections
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
The app binary now runs the client: `runtime.rs` starts `ClientHandle` (own 2-worker runtime) and installs the engine hooks (`send_to_daemon` -> `ClientHandle::request` by serverId/entryId with the client's `SendError` mapped; `daemon_link` Connect/Restart/Disconnect/Remove with the password converted to the client's `Secret`); `feed_pump.rs` forwards each event batch as one internal `feed.ingest` request (a rejected batch is logged with its count and dropped, the pump continues); `view_emitter.rs` is the pure 100 ms coalescer (injected ms clock: emit now if the window is free, otherwise a trailing emit at the window boundary); `owner.rs` is the owner loop (recv with the emitter's deadline, view emitted only when it changed, final flush); `io_worker.rs` is the single I/O worker (debounced config writes, keychain calls); `bridge.rs` keeps only the webview bridge. New `--headless` mode (needs `--control`) runs everything but the window. Shell: `Request::Internal` (engine command run without touching `ui.status`; not reachable from the webview or socket). `logging.rs`: third-party targets are capped at WARN whatever the setting (tungstenite logs the bearer subprotocol at TRACE), with a test.

## Files created / changed
| File | Change |
|------|--------|
| `apps/pistudiodashboard/src-tauri/src/{runtime,feed_pump,view_emitter,io_worker,owner,pipeline_tests}.rs` | created |
| `.../src/{main,bridge,services,logging}.rs`, `Cargo.toml` | rewritten / modified (client, config, model, keyring, tokio deps; tokio-tungstenite/futures-util/tempfile dev-deps) |
| `crates/shell/src/{lib,control}.rs` | `Request::Internal`, `run_internal`, `settle` |
| `crates/engine/src/services.rs` | `LinkCommand{Connect,Restart{entry,password},Disconnect,Remove}`, `random` hook |

## Deviations
- The tauri window is not wired through a Tauri-side owner struct any more: `owner::run` takes a view sink closure (Tauri emit, or a no-op when headless). The engine's `send_to_daemon` hook has no token argument; mutations pass an empty token (sprint-007 adds tokens for `RespondUi`/`Interrupt`).
- The pipeline test reuses the client's fake daemon via `#[path]` include (no copy); the fixture path of its handshake helper is relative to the client crate, so the test supplies the vendored `status.server_info` frame itself.
- `capabilities/default.json` and the CSP are unchanged (still `core:default`, `connect-src ipc: http://ipc.localhost`).

## Build & test results
```
$ cargo test -p pistudiodashboard              # 29 passed (emitter, io worker, logging, startup, detect, control server, 5 pipeline tests)
$ cargo clippy -p pistudiodashboard ... -D warnings   # clean
```
Pipeline tests: snapshot from the fake daemon reaches the monitor, a view is emitted and a `agent_session_stats_request` reaches the daemon; unknown serverId -> `NotConnected`; malformed batch rejected, next applies (revision +1); 1,000 batches in a burst emit <= elapsed/100 ms + 2 views and the trailing emit carries the final revision; closed port stays unconnected.

## Acceptance criteria
- [x] snapshot batch -> agents in state, a view emitted (pipeline test); one revision per batch is the engine's (feed_cmds tests)
- [x] 1,000 batches: at most ~10 views/s and last state emitted (pipeline + emitter unit test)
- [x] a click mid-window waits at most one window (emitter test)
- [x] unknown serverId -> NotConnected; read request on a live connection reaches the daemon
- [x] malformed batch: untouched state, pump/next batch fine
- [x] capabilities/CSP unchanged

## Smoke against pi-studio
Against the real dev daemon (127.0.0.1:6790, mock provider) with the headless app: `PI_STUDIO_LISTEN=127.0.0.1:6790 ./target/debug/pistudiodashboard --headless --control 7878 --control-token-file /tmp/pistudio-smoke/control.token --config-dir /tmp/pistudio-smoke/cfg`, driven over the control socket (`ui.inspect`, `engine.execute`). The app connected (probing, connecting, handshaking, syncing, live in the log), `document.inspect` showed the daemon chip `live` with 11 agents (`11 of 11 agents` covered: stats requests were sent and answered), 4 agents with pending dialogs raised as attention (dot `waiting`). A new agent made with the pi-studio CLI (`run "smoke new agent"`) appeared as a card within a second (12 cards). The window itself was not opened (no display; `--headless` instead).

## Follow-ups
- Mutation tokens for `send_to_daemon` (sprint-007). View emits in headless mode go nowhere (no webview).
