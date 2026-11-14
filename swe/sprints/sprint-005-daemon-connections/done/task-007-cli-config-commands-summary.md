# Task 007 — CLI offline config commands — Summary

- **Sprint:** sprints/sprint-005-daemon-connections
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`pistudiodashboard-cli`: `config path|show|validate [FILE]` and `daemon list|add|remove`, all offline (the crate depends on engine + config only; no client/tokio, asserted by a test), JSON lines on stdout, exit 1 on a failed operation, 2 on usage errors. `--config FILE` (global) replaces the app's file (writes need it to be named `config.json`; the atomic write owns the directory). `validate` parses read-only (no `.bak` rename, no rewrite) and prints `{path, problem, kind}` per issue, exit 1 when any. `show` prints settings and entries (no password exists in the file; a hand-written `password` key is dropped by the parser). `add` normalises the host and validates with `config::validate_entry` and the 50-entry cap, then `config::save`; `remove` notes that a keychain password is removed by the app; a newer-version file is never modified. `commands` lists `daemon.connect`/`daemon.disconnect` as `enabled:false` with "no live connection in the CLI" (`daemon.add/edit/remove/setPassword` stay enabled).

## Files
`apps/pistudiodashboard-cli/{Cargo.toml,src/main.rs,src/offline.rs,tests/cli.rs}`, `docs/development.md`.

## Deviations
- `dialog.respond` does not exist yet (sprint-007), so the disabled-reason test uses `daemon.connect`.
- `--port 70000` is rejected by clap (a `u16`): exit 2 (usage), nothing written; `--port 0` exits 1 with the field error.

## Build & test results
`cargo test -p pistudiodashboard-cli`: 8 passed (validate with issues + corrupt + missing, show without secrets, add/list/remove round trip with failures writing nothing, newer version untouched, disabled reasons, no client crate, usage errors); clippy clean.

## Acceptance criteria
All five met (see tests); the `--port 70000` case is exit 2 instead of 1 (see deviations).

## Smoke against pi-studio
Against the config file written by the running app (task-002..004 smoke): `config path` (the `--config` path; with `XDG_CONFIG_HOME` set it resolves to `$XDG_CONFIG_HOME/com.avatsaev.pistudiodashboard/config.json`), `config show` (settings line + the `Local` entry, no password key), `config validate` exit 0, `daemon list` printed `Local 127.0.0.1:6790`; a hand-made file with `port: 0` printed `{"kind":"dropped_entry","path":"daemons[0].port","problem":"port must be an integer 1-65535"}`, exit 1, no `.bak` created. The CLI opens no connection to the daemon.
