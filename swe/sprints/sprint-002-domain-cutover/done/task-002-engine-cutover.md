# Task 002 — Engine cutover: `Session` over `MonitorState`, `feed.ingest`, effects, `EngineServices`

- **Sprint:** sprints/sprint-002-domain-cutover
- **Status:** done
- **Type:** refactor
- **Area:** L5 `crates/engine`, L6 `crates/shell` (compile fixes + toast sink), `apps/pistudiodashboard-cli`, app binary compile fixes, `xtask` tables
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-001, sprint-001-core-crates/task-004

## Goal
Replace the sample checklist domain end to end in the Rust core: `Session` holds `Arc<MonitorState>` (publish without history), the `item.*`/`file.*`/`edit.*` commands and the `doc`/`ops`/`format` crates are deleted, `feed.ingest` folds `DaemonEvent` batches through `ingest` and executes the returned `Effect`s, `EngineServices` exists with all hooks optional, and `document.inspect` returns the monitor view — with `cargo xtask ci` green.

## Context / why
This is the pivot of the sprint (follow `av-rust-desktop-design/references/replace-sample-domain.md` steps 1–4 and 6–7, with the tree building and the gate green at the end). The scaffold's mutation invariant ("a failed or panicking command leaves state untouched") is kept via clone-then-publish; only history/undo, files and the picker go away (D2, D8; app-structure § What replaces the sample domain). Everything after this sprint (daemon commands, client wiring, agents, dialogs) is a command or an effect on top of this seam.

## Scope references
- `swe/specs/architecture/app-structure.md` § What replaces the sample domain (incl. its TODO(verify) on `Session::edit` → `Session::update`), § Command registry (`feed.ingest` row, rule "internal"), § Services hooks (`EngineServices` table), § Threads (Effects table, `feed.ingest` non-recording), § CLI scope, § Error handling & edge cases
- `swe/specs/architecture/live-state.md` § Engine surface (L5), § Flow, § Acceptance criteria ("one revision bump per batch")
- `swe/specs/features/agent-monitor.md` § View shape (the `document.inspect` result)
- `swe/grills/daemon-monitoring-dashboard.md` D2, D8, D28, D36
- Files to change, as they exist today:
  - `crates/engine/src/{lib,session,commands,query_cmds,params,error}.rs`; delete `item_cmds.rs`, `file_cmds.rs`, `edit_cmds.rs`; add `feed_cmds.rs`, `services.rs`, `view.rs`
  - `crates/engine/Cargo.toml`, root `Cargo.toml` (`[workspace.dependencies]`), `Cargo.lock` (must be refreshed and committed: gates use `--locked`)
  - `crates/engine/tests/panic_hunt.rs`, `crates/testkit/src/lib.rs` (`adversarial_params`)
  - delete `crates/doc`, `crates/ops`, `crates/format`; edit `xtask/src/layers.rs` (`TABLE`, docs comment, `WASM_CRATES`)
  - `crates/shell/src/{lib,state,services,menus,control,tests}.rs`, `crates/shell/examples/view.rs`
  - `apps/pistudiodashboard-cli/{Cargo.toml,src/main.rs,tests/cli.rs}`
  - `apps/pistudiodashboard/src-tauri/{Cargo.toml,src/main.rs,src/services.rs,src/crash_guard.rs,capabilities/default.json}`
  - `crates/protocol/src/outbound.rs` (new, minimal — see Notes)

## What to build
**Session (`session.rs`).** `Session { monitor: Arc<MonitorState>, revision: u64, services: EngineServices, pending_toasts: Vec<Toast> }`. No `Option`, no `DocState`, no history, no path/dirty.
- `Session::new()` (empty monitor, default services), `Session::with_monitor(MonitorState)` (tests/panic_hunt/fixtures), `with_services(EngineServices)`.
- `snapshot() -> Arc<MonitorState>`; `revision() -> u64`.
- `update<R>(&mut self, f: impl FnOnce(&mut MonitorState) -> Result<R>) -> Result<R>`: clone the monitor, run `f` on the copy, publish a new `Arc` and bump `revision` by one **only on `Ok`** (replaces `Session::edit`; contract unchanged minus history). Keep `execute()` as is: unknown id, non-object params, `enabled` gate, `catch_unwind`; reword the panic message (“the state is unchanged”).
- Remove `undo`, `redo`, `open_document`, `open_bytes`, `close_document`, `mark_saved`, `active`, `DocState`.

**Commands.** Delete `file_cmds.rs`, `edit_cmds.rs`, `item_cmds.rs` and their `v.extend` lines; remove `has_doc`; `CommandSpec` gains `internal: bool` (all other specs `false`). Add `feed_cmds.rs` with `feed.ingest` (`internal: true`, no menu/shortcut, params doc `{"events": DaemonEvent[]}`, `enabled: always`): params are deserialised with serde into `Vec<DaemonEvent>`; a malformed batch (wrong types, unknown kind, bad shape) → `EngineError::BadParams` with state untouched. Body: one `update` that runs `ingest::apply_batch(&mut working, events, now)` using each event's `at` as `now` (no clock in L5), then **after publishing** hands the returned effects to the executor. Result JSON: `{"applied": n, "effects": [<kind strings>], "skipped": [{"effect": "...", "reason": "..."}]}`. Non-recording by construction.
- `query_cmds.rs`: `command.list` rows now include `"internal": bool`; `document.inspect` (enabled always) returns the monitor view built in `view.rs` (below).

**`EngineServices` (`services.rs`).** `#[derive(Default)] pub struct EngineServices` with every hook optional, called on the owner thread, non-blocking:
- `send_to_daemon: Option<Box<dyn FnMut(&str, OutboundRequest) -> Result<(), SendError>>>` with `SendError { NotConnected, QueueFull, NotAllowed }`;
- `daemon_link: Option<Box<dyn FnMut(&str, LinkCommand) -> Result<(), String>>>` with `LinkCommand { Connect, Disconnect, Reconfigure(config::DaemonEntry), Remove }`;
- `keychain: Option<Box<dyn Keychain>>` (`get/set/delete(entry_id)` → `Result<_, KeychainError{Unavailable, Denied, NotFound}>`; secrets never `Debug`-printed);
- `config_store: Option<Box<dyn ConfigStore>>` (`load() -> LoadOutcome`, `save(&Config) -> Result<(), String>` using `crates/config` types);
- `open_url: Option<Box<dyn FnMut(&str) -> Result<(), String>>>`;
- `notify: Option<Box<dyn FnMut(Notification)>>` with `Notification { title, body, target: NotifyTarget { server_id, item_id } }`.
Define the types and the plumbing only; callers (`daemon.*`, `dialog.respond`, …) arrive in later sprints. A helper `fn require_hook(&self, name) -> Result<&mut …, String>` returns the "disabled" reason (`"no live connection in this build"` for send/link hooks etc.) for use by `enabled` fns later.

**Effect executor (`feed_cmds.rs` or `effects.rs`).** `Session::execute_effects(Vec<Effect>) -> Vec<Skipped>` exhaustively matching `Effect`:
- `Toast{kind,text}` → pushed to `pending_toasts` (L6 drains it into `UiState.toasts`, below);
- `CloseDrill{reason}` → `update` clears `monitor.drilldown` and queues an info toast with the reason;
- `FetchStats{server_id,agent_id}` → `send_to_daemon(server_id, OutboundRequest::SessionStats{agent_id})`; `FetchTimeline{server_id,agent_id}` → `send_to_daemon(server_id, OutboundRequest::FetchTimeline{agent_id})` (one logical load; the client pages `before`/`after` itself, D39). The minimal `crates/protocol/src/outbound.rs` with just these two read variants is created here; sprint-004/task-004 extends it.
- `Notify`/`ResolveNotification` → `notify` hook with a fixed-vocabulary `Notification` (title = agent title or "Agent", body = `"<daemon name>: needs input"` / `"…: failed"`; never dialog text); the toggle/coalescing/content policy is sprint-008/task-001.
- A missing hook, or a hook `Err`, never fails the command: the effect is skipped and reported in `skipped` with a reason. No panics.
`Session::take_toasts() -> Vec<Toast>` for the shell.

**Monitor view (`view.rs`).** `document.inspect` params (all optional): `{filterDaemons: string[] /*serverIds*/, filterStatuses: string[], gridWindow: {start, count}, railCollapsed: bool}` (the shell passes its `UiState` values from task-003; defaults = no filter, window = all, not collapsed; unknown or ill-typed values → `BadParams`). Result (camelCase, the agent-monitor view shape): `{revision, daemonChips[{entryId,serverId?,name,conn,staleSince,selected,agentCount}], statusChips[{bucket,count,selected}], totals{tokens,cost,covered,of,label:"N of M agents"}, cards{total, virtualised: total > 100, window:[Card]}, drill: null | {…}, railCollapsed}` built only from `ingest::{card_order, usage_totals, daemon_status_counts}` and `MonitorState` (`serverId` is an additive field: filters are by serverId, a chip without one is not selectable). `Card` fields per spec (`dot` from the status table in agent-monitor § Status dot mapping incl. attention override; `dimmed` for closed; `stale`; `currentTool`, `lastOutputLine`, `usage` with `takenAt` epoch ms — the "taken ago" string is a frontend concern since the engine has no clock). The window is the clamped slice of the ordered cards. With an empty monitor the result is empty lists and `totals.label` "0 of 0 agents".

**Shell compile fixes (L6, minimal; the full `UiState`/menu work is task-003).**
- `services.rs`: delete `PickKind`/`PickFileFn`; `Services` becomes an empty `#[derive(Default)] pub struct Services {}` (tray/config_dir/detect_local_daemon arrive in sprints 005/008). `lib.rs` stops re-exporting `PickKind`.
- `menus.rs`: delete `needs_picker`, `NO_PICKER`, the `file.*` branch of `invoke`; remove the `view.hideDone` shell command. Leave `MENU_ORDER`, `view.theme.*` and menu plumbing alone (task-003 replaces them); File/Edit/Item simply have no commands.
- `state.rs`: remove `hide_done`; add `toasts: Vec<ToastView{id: u64, kind, text}>` (bounded to 5, oldest dropped; serialised in `ui.inspect`; read-only through `ui.set`) and a private id counter; `AppShell::run` drains `session.take_toasts()` into `ui.toasts` after every command.
- `control.rs`: `ui.set` accepts only `theme` until task-003; update module docs; `view()` uses `document.inspect` (monitor view) — pass defaults now.
- `tests.rs`: rewrite against the new core (see Test plan). `examples/view.rs` (used by `npm run snapshot`): print a view built from a seeded `MonitorState` (one daemon, a few agents) instead of running `item.add`; keep the example compiling and runnable.

**CLI.** `apps/pistudiodashboard-cli`: keep `commands [--filter]` only (print `command.list` rows, incl. `internal`); delete `info`/`run` and the `tempfile` dev-dependency if unused; rewrite `tests/cli.rs`; update the doc comment and `--help` text. The CLI `Session` has no hooks.

**Crates and tables.** Delete `crates/doc`, `crates/ops`, `crates/format`; remove them from `[workspace.dependencies]`, `TABLE` and `WASM_CRATES`; engine depends on `protocol`, `model`, `ingest`, `config` (+ `testkit` dev). Update the `TABLE` doc comment table in `layers.rs` to the new layer descriptions. Remove `tauri-plugin-dialog` from `[workspace.dependencies]`, `src-tauri/Cargo.toml` and `.plugin(tauri_plugin_dialog::init())` in `main.rs`; `services.rs` of the app returns `Services::default()`; update the stale doc text in `capabilities/default.json` and `crash_guard.rs` (“your open document is unchanged”). Refresh `Cargo.lock`.

**Fuzz seed.** `crates/engine/tests/panic_hunt.rs`: `fresh()` builds a seeded `Session::with_monitor(…)` — one daemon slice, a couple of agents (running, error), one dialog item, a drill buffer — so the deepest commands run. `crates/testkit/src/lib.rs` `adversarial_params()`: replace the checklist keys (`id`, `text`, `done`, `path`, `title`, `filter`) with the new ones (`events`, `entryId`, `serverId`, `agentId`, `requestId`, `filter`, `filterDaemons`, `filterStatuses`, `gridWindow`, `railCollapsed`), keeping the six shapes (empty, wrong types, `u64::MAX`, `1e300`, 1 MiB string, NUL/`..`/RTL strings, arrays/objects where scalars are expected) and adding a hostile `events` array (oversized `connEpoch`, negative `seq`, wrong `kind`, 1 MiB strings, 10,000-element array).

## Out of scope
- `UiState` fields other than `toasts`, `ui.set` validation, `view.*` commands, `MENU_ORDER` → task-003.
- Frontend (`types.ts`, components, `test-support.ts`) → task-004. Between this task and task-004 the webview draws an outdated view shape; the gate (types are hand-written, tests use `sampleView()`) stays green.
- `daemon.*`, `agent.*`, `dialog.respond`, `settings.*` commands; the notification policy; real `OutboundRequest` mutations and allowlist → sprints 005, 006, 007, 008 and sprint-004/task-004.
- App-binary feed pump, client runtime, view coalescing → sprint-005/task-001.
- CLI offline config commands (not scheduled; see sprint-001 task-004 Notes).
- Docs → task-005.

## Acceptance criteria
- [ ] Given the new `Session`, when `update` runs a closure that returns `Err` or panics (via `execute` of a test command), then the published `Arc<MonitorState>` is the same pointer and `revision` is unchanged; a later command still works.
- [ ] Given a `feed.ingest` batch of N events, when it succeeds, then `revision` increased by exactly 1, no history structure exists, and a reader holding an earlier `snapshot()` still sees the old state. (live-state AC "one revision bump")
- [ ] Given `feed.ingest` with a malformed batch (`{"events":"x"}`, an event with a wrong-typed field, `params` not an object), then the result is an error and state/revision are untouched.
- [ ] Given a `Snapshot` batch for a daemon slice with a running agent and no recorded usage, when executed with a recording `send_to_daemon`, then one `SessionStats` request for that agent is sent after the state is published; with no hook configured, the command still succeeds and `skipped` names the effect and a reason.
- [ ] Given a hook that returns `SendError::NotConnected`, then the command succeeds, the effect is reported in `skipped`, and no panic occurs.
- [ ] Given a `CloseDrill` effect, then `monitor.drilldown` is `None` afterwards and a toast with the reason is queued; `AppShell::run` moves it into `ui.toasts` (max 5, oldest dropped).
- [ ] Given `engine.commands`, then `feed.ingest` is listed with `internal: true`, no menu entry exists for it, and `item.*`, `file.*`, `edit.*` ids are unknown (`unknown command`).
- [ ] Given an empty session, `document.inspect` returns empty chips/cards with `totals.label == "0 of 0 agents"`; given a seeded monitor with 150 agents, then `cards.total == 150`, `virtualised == true` and `cards.window.len()` equals the requested clamped window; the card order is the `ingest::card_order` order.
- [ ] Given `ui.inspect`, then it contains `ui` (no `hideDone`), `document` (the monitor view), `menus` and `shellCommands`; given the CLI, `pistudiodashboard-cli commands --filter feed` prints the `feed.ingest` row, and `info`/`run` are usage errors (exit 2).
- [ ] `panic_hunt` (the gate runs it with `--ignored`) passes with the new seed and adversarial keys; `cargo xtask layers` passes with `doc`/`ops`/`format` gone; `cargo xtask wasm` builds engine and shell.

## Test / verification plan
- Build: `cargo build --workspace --locked` (after refreshing `Cargo.lock`).
- Tests: rewrite `crates/engine/src/session.rs` unit tests (failed/panicking command untouched; params must be an object; snapshots stable; revision +1 per batch, no history); `crates/engine/src/feed_cmds.rs` tests (malformed batch, effects executed with a recording hook, hooks missing, `SendError` path, `CloseDrill`, toasts); `view.rs` tests (empty, seeded, clamped window); `crates/shell/src/tests.rs` (view = `ui` + monitor `document` + generated menus; failed command reaches status, success clears; `ui.set` theme works and unknown field is refused; toasts reach `ui.toasts`; requests from a channel are answered in order); `apps/pistudiodashboard-cli/tests/cli.rs` (`commands --filter`, usage error exit 2, `feed.ingest` internal flag). Remove tests of deleted behaviour; add no tests for undo/files.
- Focused: `cargo test -p pistudiodashboard-engine -p pistudiodashboard-shell -p pistudiodashboard-cli`; `cargo test -p pistudiodashboard-engine --test panic_hunt -- --ignored`.
- Manual: `cargo run -p pistudiodashboard-shell --example view` prints a monitor view.
- Gate: `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- **Outline problem (needs Main's awareness):** `EngineServices.send_to_daemon` takes `OutboundRequest`, which the outline only creates in sprint-004/task-004. To avoid a stub, this task adds `crates/protocol/src/outbound.rs` with just the read variants the effects need now (`SessionStats{agent_id}`, `FetchTimeline{agent_id}` (D39: one logical load; the client pages `before`/`after` itself, so no cursor/direction/limit), with an exhaustive `wire_type()`), and sprint-004/task-004 extends the enum (remaining reads, `RespondUi`, `InterruptAgent`, `MUTATION_ALLOWLIST`, `class()`, encoder). No encoder exists yet; nothing can be sent.
- The effect executor lives partly in L5 and partly in L6: the engine cannot touch `UiState` (L6), so toasts cross via `Session::take_toasts()` drained by `AppShell::run`; `CloseDrill` clears `MonitorState.drilldown` (engine-owned).
- TODO(verify) (app-structure.md): `Session::edit` → `Session::update` is the chosen simplification (no further simplification).
- TODO(verify) (app-structure.md): `notify` as an engine hook vs a shell hook — implemented as an engine hook per the spec table; the behavioural contract (owner thread, non-blocking) is identical if it moves.
- Gotcha: `cargo build --workspace --locked` fails if `Cargo.lock` still lists the deleted crates; regenerate with a plain `cargo build` first.
- `Cargo.toml` workspace lints forbid unwrap/expect/panic in non-test Rust (rule 2); the new view/effect code handles every `Option`/`Result`.
