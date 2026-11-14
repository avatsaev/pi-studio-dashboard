# Task 003 — Keychain passwords and the password prompt — Summary

- **Sprint:** sprints/sprint-005-daemon-connections
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
Engine: `Secret` (zeroising `Vec<u8>`; `Debug`/`Display` print `***`), the process-lifetime password map, `password_for` (memory, then keychain hook; locked/denied => none + `keychain_unavailable` flag), `daemon.setPassword {entryId, password, remember}` (validation: <= 1,024 bytes and token characters, else `invalid_password`; `remember` writes the keychain, failure falls back to memory with the warning `keychain_unavailable`; empty + remember deletes), passwords in `daemon.add`, keychain delete on `daemon.remove` (failure => toast, entry still removed), `redact_params`/`redact_request` (used by the app's debug request log). Prompt: a `ConnState{auth_failed}` event queues a `PasswordPrompt`; the shell opens `UiState.dialog = {kind:"password", entryId}` (queued while another dialog is open; the chip carries `keychainUnavailable` for the "it won't be saved" text). App: `keychain.rs` (`keyring` 4.2, service `com.avatsaev.pistudiodashboard`, account `daemon:<entryId>`, a `Store` trait so tests use stubs, error mapping NoEntry/NoStorageAccess/other), `io_worker` runs keychain calls on the worker, each on a helper thread with a 3 s timeout (a wedged backend yields `Unavailable`, never a hang).

## Licence / dependency check (golden rule 9)
`keyring 4.2.0` (MIT OR Apache-2.0), default feature `v1`: macOS Keychain, Windows Credential Manager, Linux Secret Service over pure-Rust `zbus 5.19` (no libdbus, no OpenSSL). Its tree (keyring-core 1.0.1, zbus-secret-service-keyring-store 1.0.1, secret-service 5.2, zbus*, ...) was read from `cargo metadata`: MIT / Apache-2.0 / Unlicense only; the MPL-2.0 crates in the app's tree (cssparser, selectors, dtoa-short, option-ext) come from Tauri, not from keyring. No deny list exists in the repo. Only the app binary depends on it (FORBIDDEN_DEPS), `cargo xtask layers` and `wasm` pass.

## Files changed
`crates/engine/src/{daemon_cmds.rs,services.rs}`, `crates/shell/src/lib.rs`, `apps/pistudiodashboard/src-tauri/src/{keychain.rs,io_worker.rs}`, workspace `Cargo.toml` (one `keyring` line), `Cargo.lock`.

## Build & test results
Engine tests: remember in keychain (service/account via the fake store, none in config), locked keychain falls back to memory with the warning and nothing on disk, connect without password then 401 raises the prompt (`keychainUnavailable: true`) and the entered password restarts the link, remove deletes keychain + memory (and a failing delete still removes), invalid passwords change nothing, `Secret` prints `***`, redaction. App tests: error mapping, worker round trip, wedged backend times out as `Unavailable` and the worker keeps answering, the real store returns an error promptly on this headless box. Shell test: auth failure opens the password dialog; removing the entry closes it.

## Acceptance criteria
- [x] remember:true -> fake keychain has `com.avatsaev.pistudiodashboard` / `daemon:<entryId>`, config has no password
- [x] unavailable keychain -> memory only + warning, nothing on disk
- [x] locked keychain at startup: connect without password, 401 raises the prompt, entered password works for the session
- [x] remove calls keychain delete and drops memory; failing delete still removes with a toast
- [x] password > 1,024 bytes or non-token characters -> `invalid_password`, nothing stored or sent
- [x] `Debug`/`Display` of `Secret` are `***`; view/config/log redaction (control request log shows `"password":"***"`)
- [ ] Manual check on a desktop session with a real keyring: not possible here (no Secret Service).

## Smoke against pi-studio
Against the real dev daemon (127.0.0.1:6790, mock provider) with the headless app: `PI_STUDIO_LISTEN=127.0.0.1:6790 ./target/debug/pistudiodashboard --headless --control 7878 --control-token-file /tmp/pistudio-smoke/control.token --config-dir /tmp/pistudio-smoke/cfg`, driven over the control socket (`ui.inspect`, `engine.execute`). There is no Secret Service on this machine, so the real keychain path was exercised in its failure mode: `daemon.add ... password` and `daemon.setPassword {remember:true}` returned `warnings:["keychain_unavailable"]` promptly (no hang), the chip showed `hasPassword:true, passwordInMemoryOnly:true, keychainUnavailable:true`, and neither `config.json` nor the app log (debug level, requests logged) contained the password (`"password":"***"`). The daemon has no password, so the 401 prompt was covered by the fake-store/fake-link tests, not live. The successful keychain write is verified with the fake store only (TODO(verify) on a desktop with a keyring).
