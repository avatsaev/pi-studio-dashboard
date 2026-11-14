# Task 002 — `settings.groupBy` config + engine save API — Summary

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`GroupBy { None, Workspace }` (with `name`/`from_name`) and `Settings.group_by` in `crates/config`; lenient parse (absent → default, no issue; unknown value or type → default + one `DefaultedValue` issue at `settings.groupBy`; the rest loads); serialised as `"none"`/`"workspace"`, no longer falls into `extra`. `Session::set_group_by` in the engine updates the in-memory config and calls `persist()` only on change (so missing store, read-only and save-failure toasts behave as for every other change). `GroupBy` is re-exported from the engine for the shell.

## Files created / changed
| File | Change |
|------|--------|
| `crates/config/src/{model,parse,lib}.rs` | `GroupBy`, `Settings.group_by`, parsing, 2 tests |
| `crates/engine/src/{daemon_cmds,lib}.rs` | `set_group_by`, re-export, 1 test |

## Build & test results
```
$ cargo test -p pistudiodashboard-config -p pistudiodashboard-engine -> config 18 passed, engine 44 passed
$ cargo clippy -p pistudiodashboard-config -p pistudiodashboard-engine --all-targets -- -D warnings -> clean
$ cargo xtask ci -> all 8 steps passed
```

## Acceptance criteria
- [x] No key → `None`, no issue (`group_by_loads_defaults_and_round_trips`)
- [x] `"workspace"` loads and survives save+reload
- [x] `"banana"`, `7`, `null`, `"Workspace"` → `None`, exactly one issue, daemon entries kept
- [x] One save per change, none for an unchanged value (`group_by_saves_once_per_change_and_respects_read_only`)
- [x] Read-only: in-memory change, no save, "newer version" toast once

## Follow-ups / TODO(verify)
- `SettingsMirror` in `crates/model` (unused by the engine today) was not extended.
