# Task 001 — Observability wire types

- **Sprint:** sprints/sprint-010-health-logs
- **Status:** backlog
- **Type:** feature
- **Area:** L0 protocol + L4 client
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** sprint-004-protocol-client/task-004

## Goal
Decode the stage 2 status/log messages, emit them as daemon events and add the four read requests to the allowlist.

## Context / why
Stage 2 consumes pi-studio's read-only observability API (flag `daemonObservability`, D30). Types must be lenient (D26); no mutation is added (D2, D37).

## Scope references
- `swe/specs/features/daemon-health-and-logs.md` § Wire, § Privacy and logging
- `swe/specs/architecture/protocol-client.md` § Requests the dashboard sends, § Mutation allowlist, § Events emitted by L4, § Lenient decoding rules
- `crates/protocol/src/` (new `observability.rs`; `outbound.rs` read variants)
- `crates/client/src/` (event emission; request correlation for the subscribe responses)
- `crates/protocol/tests/fixtures/pi-studio/` `daemon_status_*.json`, `daemon_logs_*.json`, `daemon_log_lines.json`, `status.server_info.json`

## What to build
**Types (lenient: unknown fields tolerated, unknown `transport`/`level` → `Unknown`, every `DaemonStatus` field optional)**
```
DaemonStatus = { version, startedAt, uptimeMs, pid, nodeVersion,
                 memory{rss,heapUsed,heapTotal}, cpuPercent, eventLoopLagMs,
                 agentsByStatus{<status>:n}, clients[{clientId, clientType, appVersion?, connectedAt, remoteAddress?, transport: direct|relay|Unknown}] }
LogLine = { time (epoch ms; number|ISO tolerated), level, msg, fields? }
```
- `daemon_status_subscribe_response {requestId, payload:{ok:true, intervalMs, status}}` (also `ok:false` + error); `daemon_status_update {payload: DaemonStatus}` (no requestId); `daemon_status_unsubscribe_response {requestId, payload:{ok}}`.
- `daemon_logs_subscribe_response {requestId, payload:{ok:true, minLevel, effectiveLevel, lines: LogLine[] (oldest first), dropped}}` or `{ok:false, error:"invalid_level"|"invalid_tail"}` (a normal response, not `rpc_error`); `daemon_log_lines {lines, dropped}` (no requestId, `dropped` incremental); `daemon_logs_unsubscribe_response`.
- Outbound read variants: `DaemonStatusSubscribe`, `DaemonStatusUnsubscribe`, `DaemonLogsSubscribe{minLevel, tail}` (client clamps `tail` to ≤ 1000, validates `minLevel` against pino levels trace/debug/info/warn/error/fatal), `DaemonLogsUnsubscribe`. All `class() == Read`. `daemon_status_request` is NOT used (the subscribe response carries the first snapshot) and must not be added as a variant. The mutation allowlist remains exactly two entries; the outbound allowlist test and the source-scan test are extended for the new read set.
- Client: subscribe responses correlated by `requestId` produce `RequestResult` carrying the typed payload (status snapshot / logs backlog); pushes produce `DaemonEvent::DaemonStatus{serverId, connEpoch, status}` and `DaemonEvent::DaemonLogLines{serverId, connEpoch, lines, dropped}`; ordering and `connEpoch` rules as for other events. Oversize or malformed frames follow the existing counters (never logging line content or `remoteAddress`).

## Out of scope
- `MonitorState.observability`, `health.*` commands and reducers — sprint-010/task-002.
- `logs.*` commands — sprint-010/task-003.
- UI — sprint-010/task-004/005.
- Automatic re-subscribe on reconnect — driven by the engine in task-002/003 (the client only reports connection events).

## Acceptance criteria
- [ ] Given each stage 2 fixture `frame`, when decoded with the new types, then it parses and round-trips; fixtures marked `volatile` are only compared on the non-volatile fields.
- [ ] Given a `DaemonStatus` with missing fields, an unknown `transport` and unknown extra keys, then it still decodes.
- [ ] Given the logs subscribe response `{ok:false, error:"invalid_level"}`, then it decodes to a typed failure, not a protocol error.
- [ ] Given the outbound allowlist test, then the four new variants are `Read`, `MUTATION_ALLOWLIST` is still exactly {`agent_ui_respond_request`, `interrupt_agent`}, and a `tail` over 1000 is encoded as 1000.
- [ ] Given the fake daemon, when it pushes `daemon_status_update` and `daemon_log_lines`, then the client emits `DaemonStatus`/`DaemonLogLines` events with the right `serverId` and `connEpoch`.

## Test / verification plan
- Tests: fixture decode in `crates/protocol/tests/`, allowlist test update, client test with the testkit fake daemon (subscribe → response → push). `cargo test -p pistudiodashboard-protocol -p pistudiodashboard-client`.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- The existing `daemonStatusRpc` flag is advertised but unimplemented and must be ignored (D30).
- `server_info.version` is the daemon's real version.
