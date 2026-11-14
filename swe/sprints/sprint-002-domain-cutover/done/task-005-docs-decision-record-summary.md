# Task 005 — Docs: decision record and sample-domain leftovers — Summary

- **Sprint:** sprints/sprint-002-domain-cutover
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
`docs/architecture.md`: layer table for the real tree (protocol, model, ingest, config, client, engine, shell, testkit; engine not yet wired to client), placement rules, seams (`Session::update`, service hooks, view = monitor view), Decisions table with all 15 rows of the spec's decision record. `docs/control-protocol.md`: socket examples use `view.setTheme`; `engine.commands` incl. `internal`/`layer`; `ui.inspect` shape; full `ui.set` field table as implemented; command list; daemon commands marked upcoming. `docs/development.md`, `docs/contributing.md`, `AGENTS.md` (workspace map, planned wiring noted; rules untouched), `README.md` (truthful state: empty monitor, no daemon connection). One code comment (`control_server.rs` doc example) updated.

## Files changed
`docs/{architecture,control-protocol,development,contributing}.md`, `AGENTS.md`, `README.md`, `apps/pistudiodashboard/src-tauri/src/control_server.rs` (comment only).

## Deviations
- Two decision rows were adapted for a link that cannot resolve from `docs/` (the daemon-health spec reference became plain text; "See `UiState` below" became a pointer to `crates/shell/src/state.rs`). Choices are otherwise verbatim.
- Leftover search: remaining hits are tests asserting that old ids are refused (`crates/*/tests`, `shell/src/tests.rs`), the `edit.undo` word in the Undo decision row, and `shortcuts.test.ts` fixtures; none is a stale instruction. The search covers everything outside `swe/`.

## Verification
```
$ rg -n "item\.add|item\.setDone|hideDone|file\.open|edit\.undo|\.pistudiodashboard\b|checklist|Hide Completed" (outside swe/, target, node_modules)   # only the refusal tests/fixtures above
$ cargo run -q -p pistudiodashboard-cli -- commands --filter feed        # prints the feed.ingest row (internal: true)
$ ... --example view -- --script '[["ui.set", {"theme": "system"}], ...]' # {"ok":true,"result":null}
$ cargo build --workspace --locked                                      # ok
$ cargo xtask layers / wasm; cargo clippy --workspace --all-targets -- -D warnings   # ok / ok / clean
$ cargo test --workspace                                                # 163 passed, 0 failed
$ cargo xtask ci                                                        # failed at `web`: see below
```
`cargo xtask ci` stopped at `web` because `apps/pistudiodashboard/src/tokens.test.ts` ("define the same tokens for every theme") sees 3 blocks in `tokens.css`: the design-system sprint agent is editing the frontend tokens right now; not caused by this task (docs only). All other steps were run individually and are green except `panic_hunt`/doctest, last run green in task-004's CI.

## Acceptance criteria
- [x] Decisions table has every spec row
- [x] every documented `ui.set` field exists in `crates/shell/src/fields.rs` with the same validation; the example commands exist (`view.setTheme`, `command.list`, `feed.ingest`)
- [x] no stale sample reference outside `swe/` (see deviations)
- [x] AGENTS.md workspace map matches `ls crates`, planned wiring labelled
- [x] README states the current state; run commands unchanged and valid

## Smoke against pi-studio
N/A: documentation only.
