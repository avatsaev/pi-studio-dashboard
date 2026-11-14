# Task 001 — App runtime wiring

- **Sprint:** sprints/sprint-005-daemon-connections
- **Status:** done
- **Type:** feature
- **Area:** apps/pistudiodashboard/src-tauri (app binary)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** sprint-004-protocol-client/task-005, sprint-002-domain-cutover/task-002

## Goal
Wire the client crate into the app: runtime thread, feed pump into `feed.ingest`, `send_to_daemon` and `daemon_link` hooks, a coalesced view emitter and the I/O worker thread.

## Context / why
Until now the engine and the client are separate and tested with scripts. This task makes them one running application while keeping the engine free of sockets (rule 4/7): events cross a channel and enter as one `feed.ingest` per batch; requests leave through the `EngineServices` hooks.

## Scope references
- swe/specs/architecture/app-structure.md § Threads, § Services hooks (EngineServices), § Webview boundary
- swe/specs/architecture/live-state.md § Flow
- swe/specs/architecture/protocol-client.md § Events emitted by L4 (batching), § Mutations
- `apps/pistudiodashboard/src-tauri/src/{main.rs,services.rs,bridge.rs,commands.rs,control_server.rs}`; new `runtime.rs`, `feed_pump.rs`, `view_emitter.rs`, `io_worker.rs`
- `apps/pistudiodashboard/src-tauri/Cargo.toml` (depends on `pistudiodashboard-client`)

## What to build
Modify/create in `apps/pistudiodashboard/src-tauri/src/`. Threads per the spec:
- **Client runtime**: `ClientHandle::start` once at startup; the app binary is the only crate (with the CLI-exempt tools) that depends on `pistudiodashboard-client`.
- **Feed pump thread**: blocks on the client's `Receiver<Vec<DaemonEvent>>` and forwards each batch to the owner thread as `execute("feed.ingest", {events})` through the same request channel the Tauri commands and control socket use. A rejected/malformed batch is logged (counts only) and dropped; it never stops the pump. The owner thread is never blocked by the client.
- **`send_to_daemon(serverId, OutboundRequest, token)` hook** (`EngineServices`): resolves `serverId` → the live `entryId` (the app keeps the `serverId ↔ entryId` map from `Hello`/`lastSeenServerId` events it sees on the feed; unknown/duplicate-closed → `Err(NotConnected)`), calls `ClientHandle::request` (non-blocking `try_send`), maps errors to the hook's error type. It accepts only `OutboundRequest` (type-level allowlist).
- **`daemon_link` hook**: `Connect{entry, password?}`, `Disconnect{entryId}`, `Remove{entryId}`, `Restart{entry, password?}` forwarding to `ClientHandle`. Hook absent in the CLI/tests (commands disabled with a reason).
- **View emitter**: after each owner request, if the shell's view differs from the last emitted and ≥ 100 ms passed since the last emit, emit the `view` event now; otherwise schedule a trailing emit at the 100 ms boundary so the final state is never lost; commands (clicks) are never delayed more than one window. Pure timing logic lives in a small testable struct with an injected clock.
- **I/O worker thread**: a single worker with a job queue for keychain/config-file I/O (used by tasks 002–004); results come back as a command result or a `feed.ingest`-style internal event. The main/UI thread never runs commands or blocks on I/O.
- No daemon traffic in the webview: `capabilities/default.json` stays `core:default`; CSP `connect-src` stays `ipc: http://ipc.localhost`.
- Existing sample wiring is simplified (the engine cutover left `services.rs` minimal).

## Out of scope
- Config load/save logic and `daemon.*` commands (task-002), keychain (task-003), startup + auto-detect (task-004).
- Notifications, tray, `open_url` (sprint-007/008).
- Any new view content.
- `window.focusItem` (sprint-008).

## Acceptance criteria
- [ ] Given a scripted fake daemon and the real client + engine, when a `Snapshot` batch arrives, then `engine` state contains the agents with exactly one published revision per batch and a `view` event is emitted. (D28)
- [ ] Given 1,000 `feed.ingest` batches within one second, then at most 10 `view` events are emitted and the last state is always emitted (trailing emit). (D28, D33)
- [ ] Given a command issued (click) while the emitter is mid-window, then its view update waits at most one 100 ms window.
- [ ] Given `send_to_daemon` with an unknown or disconnected `serverId`, then it returns `NotConnected` and nothing is queued; given a read request on a live connection, then the fake daemon receives it.
- [ ] Given a malformed `feed.ingest` batch, then state is untouched, the pump keeps running and the next batch applies.
- [ ] Given the Tauri config, then `capabilities/default.json` grants only `core:default` and CSP `connect-src` has no `ws:`/`wss:`/`http(s)` origin other than `ipc:`/`ipc.localhost`.

## Test / verification plan
- `cargo test -p pistudiodashboard` (app crate): unit tests for the emitter timing struct (injected clock) and an integration test with the fake daemon helpers from `crates/client/tests/support` (promote them to `testkit` or a `#[cfg(test)]` re-export if reuse requires it — do not duplicate).
- Manual: launch the app against a running pi-studio dev daemon (or the fake daemon) with the control socket (`--control`); `ui.inspect` shows agents after connect. Record result in the task summary.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- Planning note: the outline names `S002/T002` (engine cutover) as the dependency for the hooks; `EngineServices` fields are created there.
- TODO(verify) (app-structure): macOS application-menu/Quit handling and single-instance plug-in choice are not part of this task.
- The view emitter must not read the clock inside the engine/shell crates (they are not time-aware).
