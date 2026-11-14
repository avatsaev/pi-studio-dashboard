# Task 003 — `UiState` and the view commands — Summary

- **Sprint:** sprints/sprint-002-domain-cutover
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
Final `UiState` (`theme` incl. `system`, `filterDaemons`, `filterStatuses`, `attentionRailCollapsed`, `focusItem`, `gridWindow {0,100}`, `dialog`, `diagnosticsTarget`, `toasts`, `status`) with `Dialog`/`FormMode`/`FocusItem`/`GridWindow`; one shared validator per field (`fields.rs`) used by the all-or-nothing `ui.set` and by the five `view.*` shell commands (`setTheme`, `filterDaemons`, `filterStatuses`, `toggleAttentionRail` with `Cmd+Shift+A`, `setGridWindow`); param-bound menu entries `view.setTheme:dark|light|system` with one `checked`; `MENU_ORDER = Daemon, Agent, View`; `engine.commands` merges shell rows (`layer: "shell"`, engine rows `layer: "engine"`); `view()` passes the UI filters, window and rail flag to `document.inspect`.

## Files created / changed
| File | Change |
|------|--------|
| `crates/shell/src/fields.rs` | created |
| `crates/shell/src/{state,menus,control,lib,shortcuts,tests}.rs` | rewritten / extended |

## How it satisfies the scope / deviations
Per app-structure § UiState / Command registry. `MenuItem.id`/`label` became `String` (bound ids are built at runtime). Persisting theme/rail stays with sprint-005/task-002 (decision recorded in the `state.rs` module doc). The `view` example needed no change (it only seeds the monitor). Note: `document.cards.window` now defaults to the first 100 cards because `UiState.gridWindow` defaults to `{0,100}`.

## Build & test results
```
$ cargo test -p pistudiodashboard-shell        # 12 passed, 0 failed
$ cargo clippy -p pistudiodashboard-shell -p pistudiodashboard --all-targets -- -D warnings   # clean
$ cargo check -p pistudiodashboard-shell --target wasm32-unknown-unknown   # ok
$ cargo run -q -p pistudiodashboard-shell --example view | jq .ui   # shows all ten fields with defaults
```

## Acceptance criteria
- [x] `ui` has exactly the ten fields with defaults; menus `Daemon, Agent, View` (the_view_is_the_ui_state...)
- [x] theme choice round-trips (`system|light|dark`); `neon`, unknown field, bad status words refused with no field changed (ui_set_is_all_or_nothing...)
- [x] `toasts`/`status` read-only; `focusItem` clear-only (the_control_protocol..., ui_set_...)
- [x] dialog open/close; over-long id and unknown kind refused (ui_set_...)
- [x] view commands change state; `count: 100000` fails into `ui.status` unchanged; success clears (view_commands_change_ui_state...)
- [x] View menu: three theme entries, exactly one checked, `Toggle Attention Rail` `Cmd+Shift+A`; `view.setTheme:light` works (the_view_is_..., a_menu_click_runs_shell_commands)
- [x] `engine.commands` lists the five shell commands with unique ids/shortcuts (engine_commands_lists..., shortcuts test)

## Smoke against pi-studio
N/A: shell view state only, no daemon-facing surface; nothing run against 127.0.0.1:6790.

## Follow-ups / TODO(verify)
- Frontend types for the new `ui` shape and `MenuItem` (task-004).
