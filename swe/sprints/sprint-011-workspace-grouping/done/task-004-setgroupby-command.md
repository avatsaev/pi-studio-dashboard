# Task 004 — `view.setGroupBy`: UiState, View menu, `ui.set`, persistence, startup seed

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Status:** done
- **Type:** feature
- **Area:** crates/shell (L6), apps/pistudiodashboard/src-tauri (app)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-002, task-003

## Goal
The user (menu), the frontend, the CLI/control channel and automation can switch grouping with one command; the choice is held in `UiState`, sent to the monitor view, saved, and restored at startup.

## Context / why
Golden rule 3 (everything is a command) and rule 5 (UI state is data). W2/W3.

## Scope references
- `swe/specs/features/agent-workspace-grouping.md` § Commands, § State
- `swe/specs/architecture/app-structure.md` § UiState, § Command registry
- `crates/shell/src/state.rs` (`UiState`), `crates/shell/src/fields.rs` (`apply`, validators), `crates/shell/src/menus.rs` (`UI_COMMANDS`, `view.setTheme` bound-entry pattern), `crates/shell/src/control.rs` (`view()` builds the `document.inspect` params; module doc lists `ui.set` fields), `crates/shell/src/lib.rs` (`AppShell`)
- `crates/shell/src/tests.rs`, `crates/shell/src/monitor_tests.rs`
- `apps/pistudiodashboard/src-tauri/src/owner.rs` (`build_with`: `AppShell::new` runs before `startup::run`)
- `docs/control-protocol.md` (`ui.set` fields table)

## What to build
- `UiState.group_by` (serde `groupBy`, default `none`); `fields::apply` accepts `groupBy` with values `none`/`workspace`, else error `` `groupBy` is one of: none, workspace `` (state unchanged).
- Shell command `view.setGroupBy {groupBy}` in `UI_COMMANDS`: menu `View`, `bind_key: "groupBy"`, bound entries `Group: None` / `Group: By Workspace`, `checked` true on the active value, no shortcut, `enabled` always. `run`: validate + set `UiState.group_by`, then call the engine's `set_group_by` (task-002) so the config is saved. A grouping change also resets `UiState.gridWindow` to its default (the frontend scrolls to top).
- `ui.set {groupBy}` sets the UI state through the same validator and also persists (one code path for both, so a control-channel change is not lost on restart).
- `control::view` passes `groupBy` into `document.inspect`.
- Startup: after `startup::run`, seed `app.ui.group_by` from `app.session.config().settings.group_by` (an `AppShell` method, e.g. `adopt_settings()`, called from `owner::build_with`).
- Docs: `ui.set` fields table and `view.setGroupBy` in `docs/control-protocol.md`.

## Out of scope
- Frontend rendering (tasks 005–006). Persisting theme/rail.

## Acceptance criteria
- [ ] `view.setGroupBy {groupBy:"workspace"}` sets `ui.groupBy`, the next `ui.inspect` document has `groupBy:"workspace"` and a `groups` block, and the config store received one save.
- [ ] `view.setGroupBy {groupBy:"banana"}` and `ui.set {groupBy:"banana"}` fail with the reason; `UiState` unchanged; nothing saved.
- [ ] `ui.inspect` menus: View contains `view.setGroupBy:none` and `view.setGroupBy:workspace`, exactly one checked, and invoking `view.setGroupBy:workspace` reaches the same state as the command.
- [ ] Given a config dir whose file has `groupBy:"workspace"`, when `owner::build_with` runs, then `app.ui.group_by` is `workspace`; with no key, `none`.
- [ ] Switching mode resets `gridWindow` to its default.

## Test / verification plan
- `cargo test -p pistudiodashboard-shell -p pistudiodashboard` (owner test with a temp config dir — follow existing src-tauri test patterns)
- `cargo clippy -p pistudiodashboard-shell -p pistudiodashboard --all-targets -- -D warnings`
- Smoke against pi-studio: control protocol `engine.execute view.setGroupBy` against the running app/CLI on the dev daemon; record the resulting `ui.inspect` excerpt.
- `cargo xtask ci` before done.

## Notes
- TODO(verify) (spec): no shortcut proposed; add one later only if the user asks.
- The engine re-exports `GroupBy` (task-002) so the shell does not depend on the config crate.
