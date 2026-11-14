# Task 002 — Daemon commands

- **Sprint:** sprints/sprint-005-daemon-connections
- **Status:** done
- **Type:** feature
- **Area:** crates/engine (L5)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-001, sprint-001-core-crates/task-004

## Goal
Implement `daemon.add/edit/remove/connect/disconnect` with validation, config persistence via the I/O worker, `daemon_link` calls and disabled reasons.

## Context / why
How daemons get into the dashboard. The commands mutate local config only; nothing is sent to a daemon except by (re)connecting. Everything is a registry command (rule 3), so UI, CLI and agents share the entry point.

## Scope references
- swe/specs/features/daemon-connections.md § Daemon entry, § Commands, § Behavior & algorithms (validation order), § Identity
- swe/specs/architecture/config-and-secrets.md § API, § Field schema and caps, § Settings commands
- swe/specs/architecture/app-structure.md § Command registry
- `crates/engine/src/daemon_cmds.rs` (new), `crates/engine/src/{commands.rs,session.rs,params.rs,lib.rs}`
- `crates/config/` (`validate_entry`, `serialize`, entry types)
- `crates/testkit/src/lib.rs` (`adversarial_params`)
- `apps/pistudiodashboard/src-tauri/src/io_worker.rs` (config_store hook impl)

## What to build
Create `crates/engine/src/daemon_cmds.rs` and register in the command registry (ids camelCase, listed by `engine.commands`):
- `daemon.add {name, host, port, tls, webUiUrl?, password?}`: validate in order name, host, port, URL; return ALL field errors together as a list keyed by field (`invalid_name`, `duplicate_name` (case-insensitive), `invalid_host`, `invalid_port`, `invalid_url`, `too_many_daemons` > 50) using `config::validate_entry`; host normalisation (trim, lowercase, strip IPv6 brackets, reject `/ ? # @ :` outside IPv6 or whitespace); create entry with a new UUID v4 `entryId`; add the daemon slice to `MonitorState` (config order); persist via `config_store.save`; call `daemon_link.Connect`. `password` is passed to the keychain hook in task-003; in this task it is accepted, never stored in `Config`/`MonitorState`/`UiState`/view/logs and forwarded through a `password_sink` seam (no-op until task-003).
- `daemon.edit {entryId, name?, host?, port?, tls?, webUiUrl?}`: same validation (uniqueness excludes self); editing an `autoDetected` entry sets `autoDetected:false`; if host/port/tls changed → `daemon_link.Restart`; otherwise no reconnect. Unknown entry → `unknown_entry`.
- `daemon.remove {entryId}`: disconnect (`daemon_link.Remove` first, then state removal so no event for the entry applies afterwards), remove entry + slice + that daemon's agents/uiRequests/ring from `MonitorState` (unless another entry resolves to the same `serverId` — then only the duplicate mark clears), set `localDetectionDismissed:true` if the entry was `autoDetected`, persist, close the drill-down/diagnostics target if they pointed at it, clear `UiState.dialog` for the entry. The keychain delete is added in task-003.
- `daemon.connect {entryId}` (idempotent: restart only if not connecting/live; also the manual retry from `auth_failed`/`host_rejected`/`unreachable`) and `daemon.disconnect {entryId}` (stays idle until connect; not persisted).
- `enabled`/disabled reasons: connect disabled when connecting/live ("already connected"); disconnect disabled when idle; all disabled with "no live connection in the CLI" when the `daemon_link` hook is absent (connect/disconnect only); add/edit/remove remain enabled without the hook (offline config editing) but skip the link call.
- Persistence: the engine holds `Config` (from `crates/config`) in its session; mutations request `config_store.save(config)` through the hook; the app implementation debounces 250 ms, coalesces and flushes on exit and runs on the io worker. Save failure → keep in-memory config, one non-blocking toast (`Toast` effect) and retry on next change; `read_only` (newer-version) config refuses to save with a banner/toast.
- Redaction: `daemon.add`/`daemon.setPassword` `password` params are redacted wherever command params are logged (control server log, app log).
- Extend `crates/testkit` `adversarial_params` keys (`entryId`, `name`, `host`, `port`, `tls`, `webUiUrl`, `password`) and the `panic_hunt` seed so every new command is fuzzed.

## Out of scope
- Keychain storage/lookup, `daemon.setPassword`, password prompt dialog (task-003).
- Local auto-detect + startup sequence (task-004).
- `daemon.showDiagnostics` (task-006).
- UI form/dialogs (task-005).
- Identity-change/duplicate logic is the reducer's (sprint-002/task-001); this task only keeps config in step.

## Acceptance criteria
- [ ] Given the add dialog values with empty host, port 0 or 65536, blank or duplicate name (case-insensitive), or a non-http(s) `webUiUrl`, when `daemon.add` runs, then it is rejected with a per-field reason list and nothing is persisted or connected. (D10)
- [ ] Given a valid `daemon.add`, then the entry exists with a fresh UUID, `config_store.save` was called once (debounced), `daemon_link.Connect` was called with the entry, and `ui.inspect`/view contain no password.
- [ ] Given 50 entries, when adding a 51st, then `too_many_daemons`.
- [ ] Given `daemon.edit` changing only `name`, then no reconnect; changing `port`, then `Restart`; given an edit of an auto-detected entry, then `autoDetected` becomes false.
- [ ] Given `daemon.remove`, then the link `Remove` is called before state removal, the entry is gone from the saved config, the daemon's cards/attention items vanish, and an auto-detected removal sets `localDetectionDismissed:true`. (D18 delete itself lands in task-003)
- [ ] Given `daemon.connect` on a live entry, then it is disabled with a reason and a call is a no-op; on `auth_failed` it restarts.
- [ ] Given a failing `config_store.save`, then in-memory config is intact and one toast appears; given a read-only (newer-version) config, then saving is refused with a notice.
- [ ] Given adversarial params for every new command, then each answers Ok or Err without panic (panic_hunt).

## Test / verification plan
- `cargo test -p pistudiodashboard-engine` with fake `daemon_link` and `config_store` hooks recording calls (validation table test, persistence/debounce seam test, remove ordering test).
- `cargo xtask ci` includes `panic_hunt`; confirm the new commands are in its seed.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): whether the 50-entry cap, 64-char name cap and `localDetectionDismissed` flag are acceptable.
- Entry removal while connecting: task cancelled first, then deletion, no event applied afterwards (also relies on the reducer ignoring events of removed entries).
- Planning note: the outline lists `S001/T004` (config crate) as dependency for `Config`/validation.
