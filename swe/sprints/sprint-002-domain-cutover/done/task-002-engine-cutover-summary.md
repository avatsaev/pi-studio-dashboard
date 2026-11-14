# Task 002 — Engine cutover: `Session` over `MonitorState`, `feed.ingest`, effects, `EngineServices` — Summary

- **Sprint:** sprints/sprint-002-domain-cutover
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
- `crates/doc`, `crates/ops`, `crates/format` and the `item.*`/`file.*`/`edit.*` commands are deleted. `Session` holds `Arc<MonitorState>` + `revision` + `EngineServices` + pending toasts; `update` (clone, run, publish + bump only on `Ok`) replaces `edit`; no history.
- `feed.ingest` (internal): deserialises `{events}` into `Vec<DaemonEvent>` (malformed => `BadParams`, state untouched), folds them with `ingest::apply_in_place` on one working copy using each event's `at` as `now`, publishes (one revision bump), then runs the effects. Result `{applied, effects[], skipped[{effect, reason}]}`.
- `Session::execute_effects`: `Toast` -> queue; `CloseDrill` -> clear drilldown (idempotent, no extra bump) + info toast; `FetchStats`/`FetchTimeline` -> `send_to_daemon(OutboundRequest::SessionStats/FetchTimeline{agent_id})`; `Notify`/`ResolveNotification` -> `notify` hook with fixed-vocabulary text. Missing hook or hook `Err` is a `skipped` entry, never a failure.
- `EngineServices` (all hooks optional) with `SendError`, `LinkCommand`, `Keychain`/`KeychainError`/`Secret` (redacted `Debug`), `ConfigStore`, `Notification`, and `require(Hook)` for disabled reasons.
- `document.inspect` returns the monitor view (`view.rs`) from `ingest::{card_order, usage_totals, daemon_status_counts}`.
- Shell: empty `Services`; `view.hideDone` and the picker removed; `UiState.toasts` (max 5, read-only through `ui.set`) drained from `session.take_toasts()` in `AppShell::run`; `view` example prints a seeded monitor. CLI keeps `commands [--filter]`. App: dialog plugin, file arg and `Open` guard removed. `protocol::outbound` (new, minimal).
- xtask: `doc`/`ops`/`format` removed from `TABLE`, `WASM_CRATES`, doc table and unit-test fixtures. Root `Cargo.toml`: removed the three internal lines and `tauri-plugin-dialog`.

## Files created / changed
| File | Change |
|------|--------|
| `crates/engine/src/{lib,session,commands,query_cmds,error}.rs`, `Cargo.toml` | rewritten/modified |
| `crates/engine/src/{feed_cmds,services,view}.rs` | created (with tests) |
| `crates/engine/src/{item,file,edit}_cmds.rs`, `crates/{doc,ops,format}/` | deleted |
| `crates/engine/tests/panic_hunt.rs`, `crates/testkit/src/lib.rs` | new seed and adversarial keys (incl. 10,000 hostile events) |
| `crates/shell/src/{lib,state,services,menus,control,tests}.rs`, `examples/view.rs`, `Cargo.toml` | cutover |
| `apps/pistudiodashboard-cli/{Cargo.toml,src/main.rs,tests/cli.rs}` | cutover |
| `apps/pistudiodashboard/src-tauri/{Cargo.toml,src/{main,bridge,services,crash_guard}.rs,capabilities/default.json}` | cutover |
| `crates/protocol/src/outbound.rs`, `lib.rs` (2 lines) | created / extended |
| `xtask/src/layers.rs`, root `Cargo.toml`, `Cargo.lock` | tables, deps |

## How it satisfies the scope / deviations
- `OutboundRequest::FetchTimeline{agent_id}` only (D39: the client pages), not the outline's `{cursor, direction, limit}`; the stale Notes line in the task file was corrected. `protocol-client.md` still lists the cursor variant (sprint-004/task-004 owns that).
- `ConfigStore::load` returns `LoadedConfig{config, issues, read_only}` (engine type) because `config::LoadOutcome` is native-only and the engine must build for wasm32.
- `Notification` carries an `action: Raise|Resolve` field so `ResolveNotification` can use the same hook.
- `crash_guard::guard` was removed (no remaining caller); the panic hook stays.
- Status chips list only buckets with count > 0 or selected (empty monitor => empty chips); a `filterDaemons` naming no known daemon matches nothing.
- Edits to `commands.rs` and `panic_hunt.rs` were made with a scripted replace, not the edit tool.

## Build & test results
```
$ cargo build --workspace --locked                                   # ok
$ cargo test -p pistudiodashboard-engine -p -shell -p -cli -p pistudiodashboard -p -testkit -p xtask -p -model -p -ingest -p -protocol
    engine 13, shell 9, cli 2, testkit 5, xtask 8(+), model 9, ingest 10+1, protocol 13+..., all passed, 0 failed
$ cargo test -p pistudiodashboard-engine --test panic_hunt -- --ignored   # 1 passed (22 s)
$ cargo clippy -p <engine shell cli app testkit xtask protocol> --all-targets -- -D warnings   # clean
$ cargo xtask layers   # layering: ok (11 crates)
$ cargo xtask wasm     # ok (engine, shell included)
$ cargo xtask ci       # all 8 steps passed (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm)
```

## Acceptance criteria
- [x] failed/panicking command leaves the same `Arc` and revision (session tests)
- [x] batch of N events: revision +1, old snapshot unchanged (`a_batch_bumps_the_revision_once...`)
- [x] malformed batches rejected, state untouched (`a_malformed_batch_is_rejected...`)
- [x] running agent snapshot -> one `SessionStats` sent after publishing; no hook => `skipped` with reason
- [x] `SendError::NotConnected` -> command succeeds, effect skipped
- [x] `CloseDrill` clears drilldown + toast; `AppShell::run` moves toasts into `ui.toasts` (max 5)
- [x] `feed.ingest` listed `internal: true`, no menu; `item.*`/`file.*`/`edit.*` unknown
- [x] `document.inspect`: empty => "0 of 0 agents"; 150 agents => total 150, virtualised, clamped window
- [x] `ui.inspect` has `ui` (no `hideDone`), `document`, `menus`, `shellCommands`; CLI `commands --filter feed` prints the row; `info`/`run` exit 2
- [x] panic_hunt passes with the new seed; layers and wasm pass without doc/ops/format

## Smoke against pi-studio
The CLI/app cannot connect yet (no client wired; sprint-005). Instead, against the live dev daemon (127.0.0.1:6790): created agents with the pi-studio CLI (`hello`, `#stream fail`, `#ui confirm`), took `ls --json` (`list_agents_response`, 7 agents: 3 running, 1 error, 3 idle), turned it into a `feed.ingest` batch (connState, hello, snapshot) with `jq`, and fed it through the shell `view` example (`--script '[["engine.execute", {"command":"feed.ingest", ...}]]'`). Result: `applied: 3`, 7 `fetchStats` effects all `skipped` ("no live connection in this build"); view: `revision 1`, daemon chip `conn: live`, `agentCount 7`, status chips running 3 / error 1 / idle 3, `totals.label "0 of 7 agents"`, cards ordered error-first (errored), running, idle. Not exercised: pending dialogs (the daemon's `agent_ui_list_response` is not reachable from its CLI).

## Follow-ups / TODO(verify)
- `protocol-client.md` still shows `FetchTimeline{cursor, direction, limit}`; align in sprint-004/task-004.
- Frontend draws the old view shape until task-004.
