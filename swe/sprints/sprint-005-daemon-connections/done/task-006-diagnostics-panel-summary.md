# Task 006 — Diagnostics panel — Summary

- **Sprint:** sprints/sprint-005-daemon-connections
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`daemon.showDiagnostics {entryId}` (shell command, Daemon menu, Cmd+I; unknown entry -> error `unknown_entry`, `diagnosticsTarget` unchanged; menu/shortcut targets the selected or only daemon). While `UiState.diagnosticsTarget` is set the shell adds `document.diagnostics` to the view from the new internal engine query `daemon.diagnostics {entryId}` (`daemon_view::diagnostics`): state + since, last error (kind, message, time), reconnect count, connected since, messages/s, total, top-10 unknown types, unknown-enum / malformed / oversized / dropped / binary counts, RTT last + average, server (serverId, hostname, real `version`, features present/missing), duplicateOf, degraded reasons, health probe. Counters feed: `DaemonCounters.unknown_types` (top 10, most seen first, bounded names) is stored by the `Diagnostics` reducer. Never included: password, bearer, subprotocol, action/dialog text, raw frames. `DiagnosticsPanel` draws it; placement decision: **modal panel over the main content** (shared `Dialog`, new `wide` option), so the attention rail/drill stay in place; Esc, scrim click and Close send `ui.set {diagnosticsTarget:null}`; `no_observability` shows "Health and logs need a newer pi-studio daemon." inside the panel only (no grid banner).

## Files
`crates/model/src/{daemon,lib}.rs`, `crates/ingest/src/apply.rs` (+ `tests/reducers.rs`), `crates/engine/src/{daemon_view,daemon_cmds}.rs`, `crates/shell/src/{menus,control,tests}.rs`, `apps/pistudiodashboard/src/{App.tsx,lib/types.ts}`, `components/{DiagnosticsPanel,Dialog}.*`, `DiagnosticsPanel.test.tsx`.

## Build & test results
`cargo test` shell/engine/ingest/model/app all pass (new: `show_diagnostics_targets_a_known_daemon_and_ui_set_closes_it`, `the_diagnostics_view_carries_the_counters_and_never_a_secret`, `menu_clicks_open_the_forms_and_target_the_one_daemon`, `diagnostics_keep_only_the_ten_most_seen_unknown_types`); clippy clean. `npm run check`, `lint`, `test` pass (202 tests, DiagnosticsPanel 4).

## Acceptance criteria
- [x] panel shows last error, reconnects, messages/s, unknown + malformed counts, RTT and the real version
- [x] top-10 unknown types listed, nothing else; no banner elsewhere
- [x] daemon without `daemonObservability`: reason shown in the panel only (component test; engine marks `degraded`)
- [x] unknown `entryId` errors, target unchanged
- [x] slice with a password and an auth failure: neither the diagnostics view nor `ui.inspect` contains the password (shell test)
- [x] Esc / Close clear the target (component + `ui.set` test)
- [x] menu registry lists `daemon.showDiagnostics` with Cmd+I (shell test)

## Smoke against pi-studio
Same headless app as task-005. `engine.execute daemon.showDiagnostics {entryId:"nope"}` -> `unknown_entry: no daemon entry \`nope\``, target stayed null. For the dev daemon: version `0.0.115`, hostname `127.0.0.1`, real serverId, 14 features incl. `daemonObservability`, RTT 3 ms / 4 ms, 9 messages, health reachable. For `127.0.0.1:6799`: `backoff`, last error `unreachable: IO error: Connection refused (os error 111)`, reconnects 5, health unreachable, no hello so features show "—". Screenshots `done/screenshots/task-005-006-diagnostics-{live,unreachable}-{dark,light}.png` read: sections Connection / Server / Counters / Unknown types legible in both themes; the live daemon's long feature list makes the dialog scroll at 720 px height (acceptable). Not run: a daemon without `daemonObservability`, unknown message types from a real daemon.

## Deviations / follow-ups
- Diagnostics data is added by the shell via a new internal engine command rather than a `document.inspect` param, to keep the engine's monitor view independent of UI state. Updates follow the existing `Diagnostics` events (<= 1/s from the client).
- Placement TODO(verify) resolved as a modal panel; `docs/architecture.md` not changed (spec left it open).
