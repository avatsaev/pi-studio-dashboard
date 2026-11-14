# Task 003 — Keychain passwords

- **Sprint:** sprints/sprint-005-daemon-connections
- **Status:** done
- **Type:** feature
- **Area:** apps/pistudiodashboard/src-tauri + crates/engine (L5)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Store daemon passwords in the OS keychain (memory-only fallback), add `daemon.setPassword`, and prompt for the password after a 401.

## Context / why
Passwords never touch config, the webview state or logs (D18, D32). The prompt after a 401 (D25) and the fallback when the keychain is unavailable (D29) are the user-visible behaviours.

## Scope references
- swe/specs/architecture/config-and-secrets.md § Keychain and password policy
- swe/specs/features/daemon-connections.md § Commands (`daemon.setPassword`), § Behavior & algorithms (password flow), § Error messages
- swe/specs/architecture/app-structure.md § Services hooks, § Command registry
- `apps/pistudiodashboard/src-tauri/src/{keychain.rs (new),services.rs,io_worker.rs}`
- `crates/engine/src/{daemon_cmds.rs,services.rs}` (`keychain` hook trait + in-memory map)
- `crates/shell/src/state.rs` (`UiState.dialog` `{kind:"password", entryId}`)

## What to build
- **Keychain service** (app binary only, `keyring` crate): service `com.avatsaev.pistudiodashboard`, account `daemon:<entryId>`, value = UTF-8 password ≤ 1,024 bytes. Hook trait `Keychain { get(entryId) -> Result<Option<Secret>, KeychainError>, set(entryId, &Secret), delete(entryId) }` defined in the engine's `EngineServices`; the real impl runs on the io worker; a fake exists for tests (headless Linux CI must not depend on secret-service). Backend features must return an error — never hang — when no backend is available.
- **Engine in-memory map** `entryId → Secret` (process lifetime; `Secret` is a zeroising wrapper whose `Debug/Display` print `***`; never serialised; never in `Config`, `MonitorState`, `UiState`, view, `ui.inspect`, notifications or logs). The view carries only `hasPassword: bool` and `passwordInMemoryOnly: bool` per daemon chip data.
- `daemon.setPassword {entryId, password, remember}`: validate length ≤ 1,024 bytes and the token-character rule agreed in sprint-004/task-003 (`invalid_password`); `remember:true` → `keychain.set`, on success stored, on failure/unavailable → memory only + a returned warning `keychain_unavailable` ("password not saved"); `remember:false` → memory only; empty password with `remember:true` → `keychain.delete`. Then `daemon_link.Restart` with the password.
- `daemon.add` with `password`: stores through the same path (keychain if available, else memory + warning); the entry is still added on keychain failure (D18).
- **Lookup at connect** (used by Connect/Restart and startup in task-004): `password_for(entry)`: memory map first, then `keychain.get`; `ok(secret)` → use; `no_entry` → connect with none; locked/denied/error → none + flag `keychain_unavailable` (no prompt yet; connect without it, a 401 raises the prompt).
- **Prompt**: on the `ConnState{auth_failed}` event for an entry (seen during ingest) the engine sets `UiState.dialog = {kind:"password", entryId}` (the shell exposes the dialog; the form UI is task-005). Entered password goes through `daemon.setPassword`. When the keychain was unavailable the prompt text says it won't be saved ("Couldn't read the saved password — enter it to connect (it won't be saved).").
- **Remove**: `daemon.remove` also calls `keychain.delete(entryId)` (missing item fine; failure → warning toast, entry still removed) and drops the memory copy.
- **Redaction**: the control-socket/app logs redact `password` args of `daemon.add`/`daemon.setPassword`; add a log-capture test.

## Out of scope
- The password dialog UI and daemon chips (task-005).
- Local auto-detect/startup sequence (task-004).
- Orphaned-keychain-item sweeping (not in v1).
- Any change to the wire auth (sprint-004/task-003 owns the subprotocol).

## Acceptance criteria
- [ ] Given a daemon entry with a password and `remember:true`, then `config.json` contains no password and the (fake) keychain holds service `com.avatsaev.pistudiodashboard` account `daemon:<entryId>`. (D18)
- [ ] Given an unavailable or locked keychain, when `daemon.setPassword {remember:true}` runs, then the password is held in memory only, a 'not saved' warning is returned and nothing is written to disk. (D18, D29)
- [ ] Given a locked keychain at startup and a password-protected daemon, then the client connects without a password, a 401 raises the password prompt (`UiState.dialog.kind == "password"`), the entered password works for the session and nothing is written to disk. (D18, D29)
- [ ] Given `daemon.remove`, then `keychain.delete(entryId)` is called and the memory copy is gone; a failing delete still removes the entry with a warning toast. (D18)
- [ ] Given a captured view, `ui.inspect` output, app log and control-socket log during add, connect and auth-failure, then none contains the password. (D32, D29)
- [ ] Given a password over 1,024 bytes or containing non-token characters, then `daemon.setPassword` returns `invalid_password` and nothing is stored or sent.
- [ ] Given `Debug`/`Display` of a `Secret`, then the output is `***`.

## Test / verification plan
- `cargo test -p pistudiodashboard-engine` (fake keychain: ok, locked, no-entry, delete failure; log-capture redaction test) and `cargo test -p pistudiodashboard` for the real-keychain adapter's error mapping with a stubbed backend.
- Licence check (golden rule 9): `keyring` and the enabled platform backend crates (e.g. secret-service/dbus, `apple-native`, `linux-native`) — read each licence, confirm permissive, record crate/version/licence in the task summary and `ATTRIBUTION.md`/deny list if present.
- Manual (Linux desktop session with a keyring): add a daemon with a password, restart the app, confirm it connects without prompting; record result.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): `keyring` crate licence, pinned version, platform backend features (Linux headless/no secret-service must return an error, not hang).
- TODO(verify) (spec): how pi-studio's own client encodes passwords with non-token characters — the `invalid_password` rule is provisional and must be re-checked against `packages/client`.
- TODO(verify) (spec): whether to retry deletion of orphaned keychain items on next start (v1: no).
- Planning note: the shell dialog field already exists in `UiState` from sprint-002/task-003.
