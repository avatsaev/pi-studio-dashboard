# Task 004 — Config crate — Summary

- **Sprint:** sprints/sprint-001-core-crates
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
`pistudiodashboard-config` (L3): model (`Config`, `DaemonEntry`, `Settings`, ...), lenient `parse`, `validate_entry`, stable `serialize`, `load` with `.bak` recovery, atomic `save`. File I/O is `cfg(not(wasm32))`.

## Files created / changed
| File | Change |
|------|--------|
| `crates/config/Cargo.toml` | created (deps: serde, serde_json, thiserror; dev: tempfile) |
| `crates/config/src/{lib,model,parse,validate,io}.rs` | created |
| `crates/config/tests/files.rs` | created (9 temp-dir tests) |

## How it satisfies the scope / deviations
- Id source: `parse(bytes, &mut dyn FnMut() -> String)` and `load(dir, &mut dyn FnMut() -> String)`; `new_entry_id([u8;16])` builds a v4 id.
- `save(dir, &Config, read_only)` refuses with `ConfigError::ReadOnly`.
- `LoadOutcome.read_only` is also true on a read I/O error (so the unreadable file is never overwritten).
- Not done (outside "crates/config only" by parallel-work rules): `("config", Class::Layer(3))` in `xtask/src/layers.rs` TABLE — parent must add it. Workspace dependency line already existed.

## Build & test results
```
$ cargo test -p pistudiodashboard-config
15 unit passed, 9 integration passed, 0 failed
$ cargo clippy -p pistudiodashboard-config --all-targets -- -D warnings   -> clean
$ cargo check -p pistudiodashboard-config --target wasm32-unknown-unknown -> ok
```

## Smoke against pi-studio
N/A — the config crate has no daemon/network interaction.

## Acceptance criteria
- [x] all criteria 1–9 verified by `tests/files.rs` and unit tests (absent, truncated/garbage/huge + .bak replace, neon/port 70000 no .bak, 51 cap, version 2 read-only, unknown keys + password, atomic 0600 + read-only dir + tmp cleanup, validate_entry table, >1 MiB).
- [ ] `cargo xtask wasm` / `cargo xtask layers`: not run (parent gate); wasm32 `cargo check` passes; layers.rs TABLE entry pending.

## Follow-ups / TODO(verify)
- Add config to `xtask/src/layers.rs` TABLE.
- CLI `config validate/show` unscheduled (see task Notes).
