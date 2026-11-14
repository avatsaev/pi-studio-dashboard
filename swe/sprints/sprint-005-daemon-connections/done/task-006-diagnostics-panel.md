# Task 006 — Diagnostics panel

- **Sprint:** sprints/sprint-005-daemon-connections
- **Status:** done
- **Type:** feature
- **Area:** crates/shell (L6) + apps/pistudiodashboard/src (frontend)
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-005

## Goal
Add `daemon.showDiagnostics` and a read-only per-daemon diagnostics panel.

## Context / why
The engineer's explanation for 'why is this daemon not working': counters, RTT, server info and degraded reasons, without ever showing secrets or payload text (D29).

## Scope references
- swe/specs/features/daemon-connections.md § Diagnostics panel (`daemon.showDiagnostics`), § Degraded mode
- swe/specs/architecture/protocol-client.md § Diagnostics counters
- swe/specs/architecture/app-structure.md § UiState (`diagnosticsTarget`), § Command registry
- swe/specs/architecture/live-state.md § Top-level reducer (`Rtt`/`Diagnostics`)
- `crates/shell/src/{commands.rs,state.rs,view.rs,menus.rs}`
- `crates/ingest/src/` (Diagnostics/Rtt counters already stored in `DaemonSlice.counters` from sprint-002/task-001)
- `apps/pistudiodashboard/src/components/DiagnosticsPanel.tsx` (+ css, test)

## What to build
- `daemon.showDiagnostics {entryId}` (Daemon menu, Cmd+I): shell command; unknown entry → error; sets `UiState.diagnosticsTarget = entryId` (also closable via `ui.set` clear). The panel is a `UiState`-tracked panel; placement decision (TODO(verify)): render as a modal-style panel over the main content (does not replace the rail), and record the decision in the task summary and `docs/architecture.md` if it changes the spec.
- View data `view.monitor.diagnostics?` for the target: connection state + since, last error (kind, message, time), reconnect count, connected since, messages/s, total messages, top-10 unknown-type counts, unknown-enum count, malformed count, oversized-frame count, dropped events, RTT last + average, server info (serverId, hostname, **real version** from `server_info.version`, features known/missing), degraded reasons, health probe result. Updates ≤ 1/s (driven by the existing `Diagnostics` events).
- Never included: password, bearer, subprotocol, `action`/dialog payload text, raw frames. Add a view-level test that serialises the diagnostics view for a slice that went through an auth failure with a password configured and asserts the secret string is absent.
- Frontend: `DiagnosticsPanel` component draws that data in `--pi-*` tokens, Esc/close button clears `diagnosticsTarget` (`ui.set`), degraded reason `no_observability` shows 'Health and logs need a newer pi-studio daemon.'.

## Out of scope
- Health/logs panels (sprint-010).
- Persisting or exporting diagnostics.
- Chips/dialogs (task-005).

## Acceptance criteria
- [ ] Given `daemon.showDiagnostics` for a connected daemon, then the panel shows last error, reconnect count, messages/s, unknown and malformed counts, RTT and the daemon's real `version`. (D29)
- [ ] Given unknown message types counted by the client, then the top-10 by count are listed and nothing else (no banner anywhere else). (D26)
- [ ] Given a daemon without `daemonObservability`, then the degraded reason shows in the panel and no grid banner appears. (D26, D36)
- [ ] Given `daemon.showDiagnostics` with an unknown `entryId`, then it errors and `diagnosticsTarget` is unchanged.
- [ ] Given a slice that had a password and an auth failure, then neither the diagnostics view nor `ui.inspect` contains the password or any dialog text. (D29)
- [ ] Given Esc or the close button, then `diagnosticsTarget` is null.

## Test / verification plan
- `cargo test -p pistudiodashboard-shell` (command, view, redaction test); frontend component test for the panel (all sections, missing optional fields rendered as '—').
- Check the menu registry lists `daemon.showDiagnostics` with Cmd+I.
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test` pass.
- `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` in BOTH themes (dark and light); open and look at each image (golden rule 5) and record what was checked in the task summary.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (daemon-connections, app-structure): panel placement (replace rail vs dialog) — this task resolves it for implementation; note the choice.
- Counters reset on process start, not on reconnect (except `reconnectCount` semantic per spec).
