# Task 004 — Local autodetect and startup

- **Sprint:** sprints/sprint-005-daemon-connections
- **Status:** done
- **Type:** feature
- **Area:** apps/pistudiodashboard/src-tauri
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Auto-detect the local pi-studio daemon and implement the startup sequence: load config, recover from corruption, auto-add once, connect all enabled entries.

## Context / why
The first launch must be useful with no input (brief acceptance 1): a local daemon appears and its agents show up within 2 s. A corrupt config must never stop the app (D29).

## Scope references
- swe/specs/features/daemon-connections.md § Local daemon auto-detect, § Behavior & algorithms (on app start), § Error handling & edge cases
- swe/specs/architecture/config-and-secrets.md § Load and recovery, § Atomic write, § Location
- swe/specs/architecture/app-structure.md § Startup
- `apps/pistudiodashboard/src-tauri/src/{detect_local.rs (new),startup.rs (new),main.rs,io_worker.rs}`
- `crates/config/` (`load`, `save`)

## What to build
App binary (file I/O and env belong here, never in the webview or engine):
- `detect_local_daemon() -> DaemonEntry` exactly per the spec pseudocode: `home = $PI_STUDIO_HOME` (non-empty) else `<user home>/.pi-studio`; default `127.0.0.1:6767`; read `home/config.json` (cap 1 MiB, parse JSON): if env `PI_STUDIO_LISTEN` is non-empty it wins, else `json.daemon.listen` if a string; any failure (missing, unreadable, invalid JSON, wrong type) → default, no error shown; parse `host:port` incl. `[::1]:port`, parse failure → default; wildcard host (`0.0.0.0`, `::`, empty) → `127.0.0.1`. Result `DaemonEntry{name:"Local", host, port, tls:false, webUiUrl:"http://localhost:4173", autoDetected:true}`. The pi-studio files are only ever read.
- Expose it as the shell `Services.detect_local_daemon` hook (app-structure § Services hooks) and add the shell command `daemon.detectLocal {}` (app-structure § Command registry): it runs the hook and then executes `daemon.add` with the result and `autoDetected: true`, clearing `localDetectionDismissed`; disabled with a reason when the hook is absent or an auto-detected entry already exists. It is the empty-state entry point used by task-005 (everything is a command, rule 3).
- **Startup sequence** (`startup.rs`, runs on app start before the window's first view): (1) resolve the config dir (Tauri app config dir for `com.avatsaev.pistudiodashboard`; TODO(verify) exact per-OS path), `config::load(dir)`; (2) corrupt → file already renamed to `config.json.bak` by `load`, empty config, one warning toast ("Your settings file was unreadable; it was saved as config.json.bak and the dashboard started empty") also kept for diagnostics; a newer-`version` file → read-only mode with a banner/toast "settings from a newer version; changes won't be saved"; per-field issues reported; (3) auto-add rule: if no entry has `autoDetected:true` AND no entry's `host:port` equals the detected address AND `localDetectionDismissed` is false → add + persist the entry (via `daemon.add`'s internal path with `autoDetected:true`); if an auto-detected entry exists, was never edited and the detected address differs → update it; otherwise leave alone; (4) for each enabled entry: look up the password (task-003 `password_for`), `daemon_link.Connect`; (5) window opens on `main`; first `view` emit after the first ingest or after 100 ms (view emitter from task-001).
- Config dir is created `0700`, file `0600`; atomic temp+rename writes come from `crates/config`.

## Out of scope
- Keychain mechanics (task-003), commands (task-002), UI (task-005).
- The 'edit marks entry as edited' rule is in task-002.
- Notification settings and tray at startup (sprint-008).

## Acceptance criteria
- [ ] Given `$PI_STUDIO_HOME/config.json` with `daemon.listen:"0.0.0.0:7000"` and no `PI_STUDIO_LISTEN`, then detection yields `127.0.0.1:7000`; given `PI_STUDIO_LISTEN=127.0.0.1:7100`, then `7100`; given an unreadable or invalid file, then `127.0.0.1:6767` with no error shown. (D10, D29)
- [ ] Given a pi-studio daemon at the default address and an empty config, when the app launches, then an entry `Local` (`127.0.0.1:6767`, ws, `autoDetected:true`, web UI `http://localhost:4173`) exists and non-archived agents appear as cards within 2 s with no input. (D10, D15)
- [ ] Given a truncated config file and a garbage file (separately), when starting, then the original is renamed `config.json.bak`, the app starts with an empty daemon list, a warning toast is shown and the local daemon is re-detected. (D29)
- [ ] Given a config with `localDetectionDismissed:true` and no entries, then no entry is auto-created on start.
- [ ] Given a stored untouched auto-detected entry and a changed detected address, then the entry is updated; given a user-edited (`autoDetected:false`) entry, then it is left alone.
- [ ] Given a config of `version` higher than supported, then the app starts read-only and never rewrites that file.
- [ ] Given two entries at startup, then both connect without user input, each with its own password lookup.

## Test / verification plan
- `cargo test -p pistudiodashboard` — table tests for `detect_local_daemon` (env override, wildcard, IPv6, bad file, size cap) using a temp dir and injected env map; startup tests with the fake `daemon_link`/keychain hooks asserting the call sequence for each config scenario.
- Manual: run the app against a local pi-studio dev daemon with an empty config dir; confirm agents appear; record result.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): confirm `daemon/bootstrap.ts` in pi-studio also uses `overlayEnv` for `PI_STUDIO_LISTEN`, and whether `config.json` `daemon.listen` can hold forms beyond `host:port` (e.g. a unix socket path) — those fall back to the default.
- TODO(verify) (spec): Tauri 2 path-API resolution of `com.avatsaev.pistudiodashboard` per OS and the identifier in `tauri.conf.json`.
- TODO(verify) (spec): single-instance plug-in so two processes never write the config file.
- The `http://localhost:4173` default is only a default, not verified reachable.
