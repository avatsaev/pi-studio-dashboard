# Task 005 — Layer check: forbidden dependencies and explicit wasm list — Summary

- **Sprint:** sprints/sprint-001-core-crates
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
`xtask/src/layers.rs`: `TABLE` now registers `protocol` (Standalone), `model` (L1), `ingest` (L2), `config` (L3), `client` (L4, `CLIENT_LAYER`) next to the sample crates; `FORBIDDEN_DEPS` (`pistudiodashboard-client`, `tokio`, `tokio-tungstenite`, `keyring`, plus `UI_CRATES` via `is_ui_crate`) applies to every layered crate whose layer is not `CLIENT_LAYER` (data rule, testkit and exempt apps excluded), dev-deps included; `Violation::ForbiddenDep`, `WasmListing`, `WasmUnknown`; `WASM_CRATES` / `WASM_EXCLUDED = ["client"]`; `check_wasm_lists`. `xtask/src/main.rs`: `cmd_wasm` checks exactly `WASM_CRATES`; `cmd_layers` also runs `check_wasm_lists`.

## Files changed
`xtask/src/layers.rs`, `xtask/src/main.rs`.

## Notes / deviations
- A forbidden dependency is reported as `ForbiddenDep` instead of `UiBelowUiLayer` for crates in scope (no double reports); `UiBelowUiLayer` still fires for `client` (L4) using a UI crate. The old test was adjusted accordingly.
- `check_wasm_lists` only requires listing for crates that exist in the workspace, so absent `client` rows are tolerated; list entries must be layered rows in `TABLE`.
- Workspace `client` dep from L0–L3/engine/shell yields `ForbiddenDep` only (not also `Upward`).
- `ingest` is in `WASM_CRATES` (crate now exists); `client` must be added to the table row already present and stays in `WASM_EXCLUDED`.

## Build & test results
```
$ cargo test -p xtask                    # 8 passed, 0 failed
$ cargo clippy -p xtask --all-targets -- -D warnings   # clean
$ cargo xtask layers                     # layering: ok (13 crates)
$ cargo xtask wasm                       # checks protocol model ingest config doc ops format engine shell: ok
```

## Acceptance criteria
- [x] engine -> client/tokio/tokio-tungstenite/keyring/tauri-plugin-notification each `ForbiddenDep` (`engine_shell_and_core_layers_reject_forbidden_deps`, `engine_to_client_is_a_forbidden_dep_not_just_upward`)
- [x] protocol/model/ingest/config/shell likewise (same test, incl. dev-deps)
- [x] client -> tokio/tokio-tungstenite/protocol/model/ingest/config ok; client -> engine `Upward` (`client_may_use_runtime_crates_but_not_the_engine`)
- [x] real workspace `cargo xtask layers` passes and prints crate count
- [x] unlisted layered crate fails naming it (`wasm_lists_cover_every_layered_crate_exactly_once`, `real_wasm_lists_are_consistent_with_the_table`); `cargo xtask wasm` uses exactly `WASM_CRATES`
- Full `cargo xtask ci` not run (parallel work in the tree).

## Follow-ups
- sprint-002 removes `doc`/`ops`/`format` from `TABLE` and `WASM_CRATES`; sprint-004 adds the `client` crate (row and `WASM_EXCLUDED` already present).

## Smoke against pi-studio
No daemon-facing surface (build tooling only); not run.
