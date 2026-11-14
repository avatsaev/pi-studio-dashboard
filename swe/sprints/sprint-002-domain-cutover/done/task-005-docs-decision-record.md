# Task 005 — Docs: decision record and sample-domain leftovers

- **Sprint:** sprints/sprint-002-domain-cutover
- **Status:** done
- **Type:** docs
- **Area:** `docs/*`, `AGENTS.md`, `README.md`
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-004

## Goal
Make every document describe the dashboard instead of the checklist sample: copy the decision record into `docs/architecture.md`, update the `ui.set` fields and command examples in `docs/control-protocol.md`, the workspace map in `AGENTS.md`, `README.md`, and the examples in `docs/development.md`.

## Context / why
Step 9 of `replace-sample-domain.md`. Stale docs (`item.add`, `hideDone`, `.pistudiodashboard` files, `edit.undo`) misdirect agents and contributors. The decision record is a contract copied from the spec "during implementation" (app-structure § Decision record).

## Scope references
- `swe/specs/architecture/app-structure.md` § Decision record (copy verbatim as the Decisions table), § Layer / crate map (workspace map), § UiState (for `ui.set` fields), § CLI scope
- `swe/specs/overview.md` (README text)
- Real files: `docs/architecture.md`, `docs/control-protocol.md`, `docs/development.md`, `docs/contributing.md` (check for sample references only), `AGENTS.md` (workspace map + "commands" examples), `README.md`
- Code truth to read before writing: `crates/shell/src/control.rs` (`ui.set`), `crates/shell/src/state.rs`, `crates/engine/src/commands.rs`, `apps/pistudiodashboard-cli/src/main.rs`

## What to build
- `docs/architecture.md`: replace the sample Decisions table content with the app-structure § Decision record (all rows), keeping the file's existing structure/headings; update the layer diagram/crate list to the current tree (protocol, model, ingest, config exist; `client` planned; `doc`/`ops`/`format` gone).
- `docs/control-protocol.md`: `ui.set` fields exactly as implemented in task-003 (types, caps, all-or-nothing, read-only `toasts`/`status`, clear-only `focusItem`); `ui.inspect` shape (`document` = monitor view; point to agent-monitor view shape); `engine.commands` now flags internal commands; replace every `item.*`/`file.*`/`edit.*` example with `feed.ingest`/`command.list`/`view.*` examples; note that `daemon.add`/`daemon.setPassword` params will be redacted from logs (sprint-005/task-003) — mark as upcoming, not current.
- `AGENTS.md`: workspace map lists the real crates now present and the planned ones clearly labelled as planned (e.g. `client` arrives in sprint-004); golden rules untouched. Do not edit rules.
- `README.md`: what the app is (one paragraph from the overview, stating its current state: empty monitor, no daemon connection yet), how to run it (`cargo xtask ci`, `npm run app` as per `package.json`), no feature claims beyond what exists.
- `docs/development.md`: examples use the new commands (`pistudiodashboard-cli commands --filter feed`, `ui.set {"theme":"light"}` via the control socket, `npm run snapshot` example script) and drop file/sample instructions.
- Search the repo for leftovers: `rg -n "item\.add|item\.setDone|hideDone|file\.open|edit\.undo|\.pistudiodashboard\b|checklist|Hide Completed" --glob '!target' --glob '!swe/**' --glob '!node_modules'` returns nothing relevant (excluding `swe/` plan/spec text and `Cargo.lock`).

## Out of scope
- Final end-user documentation, full command list and TODO(verify) reconciliation → sprint-009/task-003.
- Documenting daemon connections, keychain, notifications (not built yet).
- Changing code, specs or the plan.

## Acceptance criteria
- [ ] Given `docs/architecture.md`, then its Decisions table contains every row of the spec's decision record with identical choices.
- [ ] Given `docs/control-protocol.md`, then each documented `ui.set` field exists in `crates/shell/src/control.rs` with the documented validation, and every example command id exists in `engine.commands`/shell commands (checked by running one example for `command.list` and one for `ui.set` against `cargo run -p pistudiodashboard-shell --example view` or the CLI).
- [ ] Given the leftover search above, then no stale sample reference remains outside `swe/`.
- [ ] Given `AGENTS.md`, then the workspace map matches `ls crates` and clearly separates planned crates.
- [ ] Given `README.md`, then it states the current state truthfully and the run commands work.

## Test / verification plan
- Run the leftover `rg` search; run the doc examples that are runnable (CLI `commands --filter feed`).
- Gate: `cargo xtask ci` passes (docs must not break `fmt`/doctest steps; code blocks in Rust doc comments are unaffected).

## Notes
- Do not copy `swe/` TODO(verify) text into the user docs; they stay in the specs until sprint-009/task-003 reconciles them.
- If the CLI help text or doc comments still mention files, fix them in this task (tiny code-comment edits are allowed; no behaviour changes).
