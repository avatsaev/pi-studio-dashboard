# Task 002 — Daemon commands — Summary

- **Sprint:** sprints/sprint-005-daemon-connections
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`crates/engine/src/daemon_cmds.rs`: `daemon.add/edit/remove/connect/disconnect` (plus `daemon.setPassword` from task-003 and the internal `config.saveFailed`). The engine holds the `Config` (`Session::apply_loaded_config`, `Session::config`); every mutation asks `config_store.save` and the `daemon_link` hook. Validation returns all failing fields at once as `bad params: {"errors":[{field,code,message}]}` (`invalid_name`, `duplicate_name`, `invalid_host`, `invalid_port`, `invalid_url`, `invalid_password`, `too_many_daemons`, `unknown_entry`) using `config::validate_entry` (+ new `config::normalise_host`). `remove` sends link `Remove` first, then drops slice, agents, dialogs, drill (unless another entry reaches the same serverId), sets `localDetectionDismissed` for an auto-detected entry, persists; the shell closes the diagnostics target and dialogs of a vanished entry (`settle`). Edit restarts only on host/port/tls change and clears `autoDetected`. `connect` is a no-op when connecting/live (restart from backoff/auth_failed/host_rejected); connect/disconnect are disabled without the link hook ("no live connection in the CLI"). The app's I/O worker debounces saves (250 ms, max 2 s), flushes on exit and reports a failed write as `config.saveFailed` -> toast; a read-only (newer-version) config refuses saves with one toast. Extras: chip fields (host, port, tls, webUiUrl, autoDetected, lastError, nextRetryInMs, hostname, version, duplicateOf, hasPassword, passwordInMemoryOnly, keychainUnavailable) in `document.inspect`; `last_seen_server_id` is stored on `Hello`; testkit adversarial keys; fuzz seed with a configured entry and a link hook.

## Files created / changed
`crates/engine/src/{daemon_cmds.rs (new),session.rs,lib.rs,commands.rs,feed_cmds.rs,view.rs,services.rs}`, `crates/config/src/{validate,lib}.rs`, `crates/testkit/src/lib.rs`, `crates/engine/tests/panic_hunt.rs`, `crates/shell/src/*` (settle), app `io_worker.rs`.

## Deviations
- `daemon.add` has no menu entry / `Cmd+N` yet: a menu click would run it without params; the form dialog (task-005, UI) adds it. Per-field errors travel as JSON inside the `bad params` text because `EngineError` carries a string.
- Entry ids come from an injectable `random` hook (the app uses getrandom; tests/CLI fall back to a deterministic counter) because the engine must build for wasm32.

## Build & test results
`cargo test -p pistudiodashboard-engine` 27 lib tests incl. 13 daemon tests (validation table, 51st entry, edit/restart, remove ordering and cleanup, delete failure, connect/auth_failed, failing/read-only save, passwords, prompts, redaction, connect_all); `panic_hunt` with the new seed passes (see report); clippy clean.

## Acceptance criteria
All eight criteria are covered by those tests (invalid fields list; valid add persists + connects once, view/config contain no password; 51st refused; edit/Restart/autoDetected; remove ordering and dismissal; connect disabled/no-op and restart after auth_failed; failing and read-only save; panic_hunt).

## Smoke against pi-studio
Against the real dev daemon (127.0.0.1:6790, mock provider) with the headless app: `PI_STUDIO_LISTEN=127.0.0.1:6790 ./target/debug/pistudiodashboard --headless --control 7878 --control-token-file /tmp/pistudio-smoke/control.token --config-dir /tmp/pistudio-smoke/cfg`, driven over the control socket (`ui.inspect`, `engine.execute`). `daemon.add` for a dead port (6999, with a password): result `{entryId, warnings:["keychain_unavailable"]}`; chip went `backoff`, `lastError.code "unreachable"`, `nextRetryInMs 2314` (reconnect scheduling works), the Local daemon stayed `live`. `daemon.add {name:"local", host:"bad host/", port:0}` -> `duplicate_name`, `invalid_host`, `invalid_port` together. `daemon.edit {port:6998}` -> `reconnected:true`; `daemon.connect` on the live entry -> `connecting:false, reason:"already connected"`; `daemon.remove` -> chip gone, cards 12 (Local's) intact, config rewritten (`Dead` absent after the debounce).
