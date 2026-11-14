# Task 004 — `view.setGroupBy` — Summary

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`UiState.group_by` (serde `groupBy`, default `none`, type = the config crate's `GroupBy` re-exported by the engine); validator `fields::group_by` shared by `ui.set` and the command (message `` `groupBy` is one of: none, workspace ``); shell command `view.setGroupBy` (View menu, bound entries `Group: None` / `Group: By Workspace`, checked on the active one, no shortcut). `AppShell::set_group_by` updates the UI state, resets `gridWindow` on a change and asks the engine to save; `ui.set` with a `groupBy` key goes through the same save (and resets the window unless the request also sets one). `control::view` passes `groupBy` to `document.inspect`. `AppShell::adopt_settings()` seeds the UI state from the loaded config; `owner::build_with` calls it after `startup::run`. `GroupBy` gained `Deserialize` for `UiState`.

## Files changed
`crates/config/src/model.rs`; `crates/shell/src/{state,fields,menus,control,lib,tests}.rs`, `crates/shell/Cargo.toml` (dev-dep config, for a fake `ConfigStore`); `apps/pistudiodashboard/src-tauri/src/{owner,pipeline_tests}.rs`; `docs/control-protocol.md`; spec wording (shell commands return messages, not `invalid_group_by` codes).

## Build & test results
```
$ cargo test -p pistudiodashboard-shell -p pistudiodashboard -> shell 30 passed (5 new in `group_by`), app incl. new startup/restart test passed
$ cargo clippy --workspace --all-targets -- -D warnings -> clean
$ cargo xtask ci -> all 8 steps passed (vitest 204 passed)
```

## Smoke against pi-studio
Headless app (`--headless --control 7878`, throwaway config dir) against the dev daemon on 6790: `ui.inspect` menus showed `view.setGroupBy:none` checked / `:workspace` unchecked; `engine.execute view.setGroupBy workspace` → `ui.groupBy: workspace`, `document.groups` labels `['solo','tmp','a/app','b/app']`; `groupBy:"x"` → `` `groupBy` is one of: none, workspace `` ; `config.json` then held `"groupBy": "workspace"` (after the 250 ms save debounce); a restarted app came up with `ui.groupBy: workspace`, the menu check on Group: By Workspace and `groups` present. Note: killing the app with SIGTERM inside the debounce window loses that last save (existing behaviour of the I/O worker; `shutdown()` flushes).

## Acceptance criteria
- [x] Command sets state, document has `groupBy` + `groups`, store received one save
- [x] `banana` refused via command and `ui.set`; state unchanged, nothing saved
- [x] Menu has both entries, exactly one checked; invoking reaches the same state
- [x] `owner::build_with` seeds `workspace` from a file, `none` without the key (`the_saved_grouping_is_restored_at_startup_and_a_change_is_saved`)
- [x] Switching mode resets `gridWindow`

## Follow-ups / TODO(verify)
- Unrelated pre-existing behaviour: SIGTERM within the save debounce loses the last setting (all settings, not new).
