# Task 007 — CLI offline config commands

- **Sprint:** sprints/sprint-005-daemon-connections
- **Status:** done
- **Type:** feature
- **Area:** apps/pistudiodashboard-cli (binary) + crates/config (L3)
- **Priority:** P3
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Give `pistudiodashboard-cli` offline commands that read and edit the dashboard config file — list, add and remove daemon entries, show settings, validate a file — without ever opening a network connection or printing a password.

## Context / why
app-structure § CLI scope keeps the CLI as the registry-automation surface and adds offline config commands; config-and-secrets acceptance requires `config validate <path>` to list issues and print no password. Sprint-002/task-002 reduced the CLI to `commands`; this task adds the rest once the config persistence path exists (task-002). Live monitoring from the CLI stays deferred.

## Scope references
- `swe/specs/architecture/app-structure.md` § CLI scope
- `swe/specs/architecture/config-and-secrets.md` § Location, § File format, § Field schema and caps, § API, § Load and recovery, § Acceptance criteria (`config validate`)
- `apps/pistudiodashboard-cli/src/main.rs`, `apps/pistudiodashboard-cli/tests/cli.rs`
- `crates/config/src/` (API from sprint-001/task-004)

## What to build
- Subcommands (clap), all offline, JSON lines on stdout, diagnostics on stderr, exit 1 on a failed operation and 2 on usage errors (scaffold convention):
  - `config path` — prints the resolved config file path (same resolution as the app; `--config <path>` overrides for every `config` subcommand).
  - `config show` — prints settings and daemon entries (`entryId`, name, host, port, tls, webUiUrl, autoDetected); never a password (none is stored in the file, D18) and never keychain contents.
  - `config validate [<path>]` — runs `crates/config` load/validation without the `.bak` rename side effect (read-only) and prints one line per issue (`{path, problem}`), exit 1 if any.
  - `daemon list`, `daemon add --name --host --port [--tls] [--web-ui-url]`, `daemon remove <entryId>` — go through the same `crates/config` validation and atomic write as the app; `daemon remove` also tells the user that the keychain password (if any) is removed by the app, not the CLI (the CLI never touches the keychain).
- The CLI's `Session` has no `send_to_daemon`/`daemon_link` hooks: `commands` lists `daemon.connect`, `agent.*`, `dialog.respond` with `enabled:false` and the reason "no live connection in the CLI".
- No new dependency beyond what the workspace already has.

## Out of scope
- Keychain access from the CLI (never).
- Live connections, watching agents, answering dialogs from the CLI (deferred, app-structure § CLI scope).
- Concurrent-writer protection between the app and the CLI beyond the atomic write (single-instance work is a TODO in config-and-secrets).

## Acceptance criteria
- [ ] Given a config file with an invalid entry (port 0, 65-char name), when `pistudiodashboard-cli config validate <path>` runs, then each issue is printed as one JSON line, the exit status is 1, the file is unchanged and no `.bak` is created. (config-and-secrets)
- [ ] Given a valid file, when `config show` runs, then settings and entries are printed and the output contains no `password` key or keychain value. (D18, D29)
- [ ] Given `daemon add --name x --host 127.0.0.1 --port 6790`, then the entry is written through the atomic write and `daemon list` shows it; given `--port 70000`, then nothing is written and the exit status is 1.
- [ ] Given any `config`/`daemon` subcommand, then no network connection is opened (the binary links no client crate; asserted by `cargo tree`-based layer rule or by the CLI not depending on `pistudiodashboard-client`). (app-structure § CLI scope)
- [ ] Given `pistudiodashboard-cli commands`, then `daemon.connect` and `dialog.respond` are listed with `enabled:false` and the reason "no live connection in the CLI".

## Test / verification plan
- Extend `apps/pistudiodashboard-cli/tests/cli.rs` (runs the built binary against a temp config dir): validate with issues, show without secrets, add/list/remove round trip, bad port refused, `commands` disabled reasons.
- `cargo test -p pistudiodashboard-cli`.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (config-and-secrets): single-instance enforcement so the app and the CLI never write the file at the same moment; the atomic write prevents torn files but not lost updates.
- Added at plan review: the outline had no task for the CLI config commands that app-structure § CLI scope and config-and-secrets require.
