# Task 002 — `settings.groupBy` in the config file and an engine API to save it

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Status:** done
- **Type:** feature
- **Area:** crates/config (L3), crates/engine (L5)
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** none

## Goal
The config file carries an optional `settings.groupBy` (`none` | `workspace`), loaded leniently and saved atomically, and the engine exposes one call that sets it and saves the config through the existing `config_store` hook.

## Context / why
W3: the grouping mode survives restarts. Plan-time finding: no UI preference is persisted yet (`theme` and `attentionRailCollapsed` are in the file but nothing writes or seeds them), so this task builds the storage half of that path; task-004 wires the shell to it.

## Scope references
- `swe/specs/features/agent-workspace-grouping.md` § State, § Data & persistence touchpoints, § Error handling (config rows)
- `swe/specs/architecture/config-and-secrets.md` § Field schema (`settings.groupBy`), § Settings commands (`view.setGroupBy`)
- `crates/config/src/model.rs` (`Settings`, `ThemeChoice` as the enum pattern), `crates/config/src/parse.rs` (`theme` parsing + `defaulted` issue), `crates/config/src/lib.rs` (exports)
- `crates/engine/src/daemon_cmds.rs` (`Session::config`, `Session::persist`, read-only handling)

## What to build
- Config: `GroupBy { None, Workspace }` (default `None`, serialised `"none"`/`"workspace"`), `Settings.group_by`. Parse: absent → default, no issue; `"none"`/`"workspace"` accepted; any other value or type → default plus a `DefaultedValue` issue at path `settings.groupBy`; the rest of the file still loads. Serialise the key so a save round-trips it. Old files without the key load unchanged.
- Engine: a public `Session` method (e.g. `set_group_by(GroupBy)`) that updates `config.settings.group_by` and, only when the value changed, calls the existing `persist()` (which already handles a missing store, read-only configs and save failures as toasts). Re-export `GroupBy` from the engine so the shell (L6) needs no direct config dependency (layer rules).
- The engine command surface is unchanged here (the user-facing command is the shell's `view.setGroupBy`, task-004).

## Out of scope
- Persisting `theme` or the rail flag (same path, separate decision).
- `UiState`, the shell command and startup seeding (task-004).

## Acceptance criteria
- [ ] Given a config without `groupBy`, then it loads with `GroupBy::None` and no issue.
- [ ] Given `{"settings":{"groupBy":"workspace"}}`, then it loads as `Workspace`; saving and reloading keeps it.
- [ ] Given `groupBy: "banana"` or `groupBy: 7`, then it loads as `None` with one `DefaultedValue` issue for `settings.groupBy`, and daemon entries in the same file still load.
- [ ] Given a fake `config_store`, when `set_group_by(Workspace)` runs, then exactly one save is requested with the new value; setting the same value again requests none.
- [ ] Given a read-only (newer-version) config, then `set_group_by` changes the in-memory value, saves nothing and shows the existing "won't be saved" toast once.

## Test / verification plan
- `cargo test -p pistudiodashboard-config -p pistudiodashboard-engine`
- `cargo clippy -p pistudiodashboard-config -p pistudiodashboard-engine --all-targets -- -D warnings`
- `cargo xtask ci` (layers + wasm) before done.

## Notes
- Follow the existing lenient-parse conventions exactly (`theme` is the template); unknown keys in `settings` already round-trip through `extra` — make sure `groupBy` is no longer left in `extra` after this change.
- Update the config file example in `swe/specs/architecture/config-and-secrets.md` only if the implemented key differs from the spec (it should not).
