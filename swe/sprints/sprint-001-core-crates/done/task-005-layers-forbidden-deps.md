# Task 005 — Layer check: forbidden dependencies and explicit wasm list

- **Sprint:** sprints/sprint-001-core-crates
- **Status:** done
- **Type:** chore
- **Area:** `xtask/src/layers.rs`, `xtask/src/main.rs` (`cmd_wasm`)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-001, task-003, task-004

## Goal
Make the architecture gate reject any dependency of L0–L3, `engine` or `shell` on `pistudiodashboard-client`, `tokio`, `tokio-tungstenite`, `keyring` or any tauri crate, and replace the wasm crate list derived from `TABLE` with an explicit list so a future `client` crate is excluded on purpose.

## Context / why
The engine must stay wasm-checkable and testable with scripted `DaemonEvent`s and no sockets (rules 4 and 7, app-structure). Today `cargo xtask layers` only enforces "lower layers only" and `cmd_wasm` checks every `Layer`/`Standalone` crate, which would try to build the tokio-based L4 `client` for wasm32. Doing this now, before the client exists (sprint-004), means the rule is in force from the first client commit.

## Scope references
- `swe/specs/architecture/app-structure.md` § Layer / crate map (paragraph "Layer numbers: L4 `client` …" and its TODO(verify)), § Wasm check scope (TODO(verify))
- `xtask/src/layers.rs` (`TABLE`, `check`, `Violation`, `UI_CRATES`, unit tests at the bottom), `xtask/src/main.rs` (`cmd_wasm`, `cmd_layers`)
- `AGENTS.md` golden rules 4 and 7

## What to build
- In `xtask/src/layers.rs`:
  - `pub const FORBIDDEN_DEPS: &[&str]` = `pistudiodashboard-client`, `tokio`, `tokio-tungstenite`, `keyring` plus the existing `UI_CRATES` patterns (reuse `is_ui_crate`, so `tauri*` stays a prefix match). Apply to every crate whose class layer is ≤ 3 or is `engine` or `shell` (i.e. everything registered except `Testkit`, `Exempt` and `client`). Define the exemption by data (`NO_ASYNC_BELOW: &[&str]` or a rule on `Class::Layer(l)` with `l != 4`), not by name-matching in `check`. Testkit and `client` may depend on tokio (the scripted fake daemon in sprint-004).
  - New `Violation::ForbiddenDep { krate, dep }` with a `Display` message that names the rule and the app-structure spec.
  - Keep the existing `UiBelowUiLayer` rule as is (engine/shell remain below app).
  - Note `client` is registered later (sprint-004/task-002) as `("client", Class::Layer(4))` — nothing to add to `TABLE` now; add a unit test with a synthetic `client` crate showing: `engine → client` is a `ForbiddenDep` (and not only "Upward"), `client → tokio` is fine, `client → engine` is an `Upward` violation (L4 → L5).
- In `xtask/src/main.rs`: replace the `TABLE` scan in `cmd_wasm` with `layers::WASM_CRATES: &[&str]` (the explicit list: `protocol`, `model`, `config`, plus today's still-registered `doc`, `ops`, `format`, `engine`, `shell`), and add `layers::WASM_EXCLUDED: &[&str] = &["client"]`. Add a check (in `cargo xtask layers`, with a unit test) that every `Layer`/`Standalone` crate in `TABLE` appears in exactly one of the two lists, so a new crate cannot be forgotten silently.
  - Later tasks that add or remove crates edit the lists: sprint-002/task-001 adds `ingest`, sprint-002/task-002 removes `doc`/`ops`/`format`, sprint-004/task-002 adds `client` to `WASM_EXCLUDED`.
- Unit tests next to the existing ones in `layers.rs`, written against the synthetic `Crate`/`Dep` model (no cargo invocation).

## Out of scope
- Creating the `client` crate or deps on tokio (sprint-004/task-002).
- Checking transitive dependencies (`cargo metadata --no-deps` only exposes direct deps; this matches the existing rule engine).
- Changing the `Exempt` apps' freedom to use any crate.

## Acceptance criteria
- [ ] Given a synthetic engine crate depending on `pistudiodashboard-client`, `tokio`, `tokio-tungstenite`, `keyring` or `tauri-plugin-notification`, when `check` runs, then each yields a `ForbiddenDep` violation.
- [ ] Given synthetic L0–L3 crates (`protocol`, `model`, `ingest`, `config`) and `shell` depending on the same crates, then each is a violation.
- [ ] Given `client` (L4) depending on `tokio`, `tokio-tungstenite` and `protocol`/`model`/`config`, then no violation; given it depending on `engine`, then an `Upward` violation.
- [ ] Given the real workspace today, when `cargo xtask layers` runs, then it passes (no existing crate violates the new rule) and prints the crate count.
- [ ] Given a `TABLE` entry of `Layer`/`Standalone` class missing from both wasm lists, when `cargo xtask layers` runs, then it fails naming the crate; `cargo xtask wasm` checks exactly `WASM_CRATES`.

## Test / verification plan
- Tests: `cargo test -p xtask` (new unit tests in `xtask/src/layers.rs`).
- Manual: `cargo xtask layers` and `cargo xtask wasm` succeed on the current tree.
- Gate: `cargo xtask ci` passes.

## Notes
- Resolves both TODO(verify)s in app-structure.md: the scaffold needed a small extension (a forbidden-dependency list) and `cmd_wasm` needs an explicit list.
- Dev-dependencies are covered by the rule too (a core crate must not pull tokio even for tests); if that proves too strict when the fake daemon arrives, relax it to non-dev deps for `engine`/`shell` only and record the decision here.
