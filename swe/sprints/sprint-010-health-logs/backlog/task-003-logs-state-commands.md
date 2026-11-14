# Task 003 — Logs state and commands

- **Sprint:** sprints/sprint-010-health-logs
- **Status:** backlog
- **Type:** feature
- **Area:** L1 model + L2 reducers + L5 engine
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Add the logs panel lifecycle (`logs.open/close/setMinLevel`) with a bounded, never-persisted line ring.

## Context / why
Daemon logs may contain sensitive data (paths, prompts): they are shown only, kept in memory while the panel is open and never logged or persisted (D29). The daemon rate-caps at 200 lines/s; the dashboard bounds memory and coalesces views (D28).

## Scope references
- `swe/specs/features/daemon-health-and-logs.md` § Logs panel (algorithm and display rules), § Privacy and logging, § Performance, § Error handling & edge cases
- `swe/specs/architecture/live-state.md` § Bounds and enforcement
- `swe/specs/architecture/app-structure.md` § Command registry
- `crates/model/src/` (`LogsState`, ring helper reuse)
- `crates/ingest/`
- `crates/engine/src/` (new `logs_cmds.rs`)
- `crates/testkit/`

## What to build
- `logs.open {serverId, minLevel? = "info"}`: same gating as `health.open` ("daemon too old"/"daemon disconnected"); closes an open logs panel first; sets `logs = {serverId, minLevel, effectiveLevel:None, lines:[], dropped:0, droppedTotal:0, lastLineAt}`; sends `DaemonLogsSubscribe{minLevel, tail:1000}`.
- Reducers: subscribe response `ok:true` → `effectiveLevel`, ingest `lines` (backlog, oldest first) like pushes, `droppedTotal += dropped`; `ok:false` (`invalid_level`/`invalid_tail`) → panel error, no retry. `DaemonLogLines` for the open `serverId`: truncate each `msg` to 4 KB and serialised `fields` to 8 KB with an ellipsis marker (count truncations in diagnostics, counts only), push to the ring, evict oldest above **5,000**, `droppedTotal += dropped`, `dropped` = most recent non-zero value for the "N lines dropped" notice, `lastLineAt` updated.
- `logs.setMinLevel {minLevel}`: clears lines and counters, sets `minLevel`, re-sends `DaemonLogsSubscribe{minLevel, tail:1000}` (re-subscribe replaces the level; no unsubscribe). Invalid level rejected locally.
- Connection lost → keep lines, banner "disconnected"; on reconnect re-subscribe with the same `minLevel`, insert a "— reconnected —" divider marker, new backlog follows (duplicates accepted). Daemon replaced/removed/disconnected by user → close.
- `logs.close {}`: send `DaemonLogsUnsubscribe` if connected; discard all lines (no copy remains in state, diagnostics, or any retained view history).
- The view carries "daemon logs at <effectiveLevel>" when `effectiveLevel` is higher than `minLevel`, with `silent` stating the daemon emits no logs, plus the dropped counter. Log text is plain strings in the view only while the panel is open.
- Privacy: no log `msg`/`fields`/`remoteAddress` text in the dashboard's own log, event ring, toasts or notifications — counts and sizes only; notification/tray code never reads the buffer. Add a test that fails if `logging.rs`-bound paths receive these strings.

## Out of scope
- Wire types — task-001; health — task-002.
- Logs panel rendering, virtualisation, auto-scroll — sprint-010/task-005.
- Persisting the last-used `minLevel` (TODO(verify)): not done.

## Acceptance criteria
- [ ] Given `logs.open` with default level, then exactly one `daemon_logs_subscribe_request {minLevel:"info", tail:1000}` is sent and the response's `lines` appear oldest first.
- [ ] Given `logs.setMinLevel {minLevel:"warn"}`, then the buffer clears, a new subscribe request with `warn` is sent, and no unsubscribe is sent.
- [ ] Given a response with `effectiveLevel` higher than `minLevel` (including `silent`), then the view carries the "daemon logs at <level>" notice (and the no-logs statement for `silent`).
- [ ] Given pushes with `dropped` 3 and 5 plus a response `dropped` 2, then `droppedTotal` is 10.
- [ ] Given 10,000 pushed lines, then the ring holds exactly 5,000 (newest) and a 1 MB `msg` is truncated to 4 KB with a marker.
- [ ] Given `logs.close`, daemon removal or app state reset, then no line remains in state and `daemon_logs_unsubscribe_request` was sent when connected.
- [ ] Given a disconnect and reconnect, then the same `minLevel` is re-subscribed and a reconnected divider precedes the new backlog.
- [ ] Given log lines containing a marker string, then that string appears nowhere in logger output, event ring, toasts or effects (test with a capturing logger).
- [ ] Given `logs.open` on a daemon without `daemonObservability`, then it is disabled with "daemon too old" and nothing is sent.

## Test / verification plan
- Tests: reducer/engine tests with recording hook and a capturing logger; ring bound test (proptest optional: arbitrary push sequences never exceed 5,000 lines). `cargo test -p pistudiodashboard-engine` and the reducers/model crates.
- `panic_hunt` adversarial keys for `logs.*`.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- TODO(verify): the 5,000-line ring, 4 KB/8 KB truncation sizes are spec proposals; confirm with the engineer.
- TODO(verify): whether `minLevel` should persist as a setting (default `info` each open for now).
