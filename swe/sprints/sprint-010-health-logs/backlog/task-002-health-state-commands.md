# Task 002 — Health state and commands

- **Sprint:** sprints/sprint-010-health-logs
- **Status:** backlog
- **Type:** feature
- **Area:** L1 model + L2 reducers + L5 engine
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-001

## Goal
Add `MonitorState.observability` and the `health.open/close` lifecycle with degraded-mode gating.

## Context / why
Subscriptions exist only while a panel is open (D29, D33) and the daemon forgets them on disconnect, so the engine must own open/close/re-subscribe and the single-panel rule.

## Scope references
- `swe/specs/features/daemon-health-and-logs.md` § Commands, § State, § Health panel (algorithm), § Degraded mode, § Error handling & edge cases
- `swe/specs/architecture/live-state.md` § MonitorState (observability), § Bounds
- `swe/specs/architecture/app-structure.md` § Command registry
- `crates/model/src/` (`Observability`, `HealthState`, `observabilitySupported`)
- `crates/ingest/`
- `crates/engine/src/` (new `health_cmds.rs`; `commands.rs`; `params.rs`)
- `crates/testkit/` (adversarial params)

## What to build
- Model: `MonitorState.observability = { health: {serverId, status?: DaemonStatus, intervalMs?, receivedAt?, supported} | null, logs: … (type declared here, populated in task-003), observabilitySupported: {serverId → bool} }`, derived from `server_info.features.daemonObservability` (absent → false; `daemonStatusRpc` alone counts as absent). Not persisted; `validate()` covers it.
- `health.open {serverId}`: disabled "daemon too old" when unsupported, "daemon disconnected" when not connected; closes any open health panel first (unsubscribe, then subscribe); sets `health = {serverId, status:None}` and sends `DaemonStatusSubscribe`.
- Reducers: subscribe response `ok:true` → store `status`, `intervalMs`, `receivedAt=now` immediately; `ok:false`/error → panel error text, subscription considered closed, **no retry loop**; `DaemonStatus` push for the open `serverId` → replace status, `receivedAt=now`; pushes for other servers ignored.
- Connection lost → keep last status and mark stale (derived from `receivedAt` vs `intervalMs`, not a hard-coded 5 s); on reconnect (same `serverId`, new `connEpoch`) with the panel still open → engine re-sends the subscribe; new `serverId` at the address, daemon removed or disconnected by the user → panel closes (unsubscribe if still connected).
- `health.close {}`: send `DaemonStatusUnsubscribe` if connected; `health = None`. Enabled only when a panel is open.
- `UiState` records which panel is open for the view (reuse `diagnosticsTarget`-style field or add `panel`; keep the right area rule: health/logs replace the rail like the drill-down, tentative per spec TODO).
- View carries derived display values (formatted uptime recomputed from `startedAt` + local clock, memory in binary units, agents by status in canonical order with unknown labelled "unknown", clients sorted self first then `connectedAt`, capped at 200 with "+N more"); "this dashboard" mark by `clientId` equal to the connection's own `hello.clientId`, else by `appVersion` prefix `pistudiodashboard/` (marked "pi-studio dashboard", not self).

## Out of scope
- Logs panel state/commands — sprint-010/task-003.
- Health panel rendering — sprint-010/task-004.
- Logs content privacy rules — task-003.

## Acceptance criteria
- [ ] Given a daemon whose features lack `daemonObservability` (including one with only `daemonStatusRpc`), then `health.open` is disabled with "daemon too old" and no request of the family is sent.
- [ ] Given a stage 2 daemon, when `health.open` runs, then only `daemon_status_subscribe_request` is sent and the subscribe response's `status` is in state immediately with `intervalMs` 5000 from the fixture.
- [ ] Given `daemon_status_update` pushes, then state refreshes; given a push for a different `serverId`, then it is ignored.
- [ ] Given `health.close`, a panel switch to another daemon, or daemon removal, then `daemon_status_unsubscribe_request` is sent (when connected) and later pushes are not applied.
- [ ] Given a disconnect with the panel open, then the last status is kept and marked stale; after reconnect the subscribe is re-sent without user action.
- [ ] Given a subscribe `ok:false`, then the panel shows the error and no retry happens.
- [ ] Given the clients list contains `clientId` equal to the connection's own id, then that row is self; given only an `appVersion` prefix match, then it is labelled "pi-studio dashboard" (other instance); given the own client missing, then no mark and no error.
- [ ] Given a recording fake, then the only requests produced are the read requests of this family and no mutation type.

## Test / verification plan
- Tests: reducer + engine tests with the recording hook (open/close/switch/reconnect/error/degraded); display-derivation unit tests (uptime format, memory units, client ordering and cap, unknown status). `cargo test -p pistudiodashboard-engine` and the reducers/model crates.
- `panic_hunt` adversarial keys for `health.open/close`.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- TODO(verify): event-loop-lag tint thresholds (≥100 ms warning, ≥500 ms danger) are proposals — carry the thresholds as engine-derived level, not frontend logic.
- TODO(verify): panel placement (tentative: right panel replacing the rail, opened from daemon chip/diagnostics menu).
- The 200-client cap is a spec proposal.
