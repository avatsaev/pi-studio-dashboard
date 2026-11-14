# Feature — Daemon Health and Logs (stage 2)

> Part of: [overview.md](../overview.md)
> Dependencies: [protocol-client.md](../architecture/protocol-client.md), [live-state.md](../architecture/live-state.md), [app-structure.md](../architecture/app-structure.md), [design-system.md](../architecture/design-system.md), [daemon-connections.md](../features/daemon-connections.md); pi-studio: [daemon-status-rpc.md](../../../../pi-studio/swe/features/daemon-status-rpc.md), [daemon-log-streaming.md](../../../../pi-studio/swe/features/daemon-log-streaming.md), [external-client-fixtures.md](../../../../pi-studio/swe/architecture/external-client-fixtures.md)
> Source brief: [daemon-monitoring-dashboard.md](../../grills/daemon-monitoring-dashboard.md) (D4, D5, D23, D26, D29, D30, D31, D33, D37; acceptance criteria 15–17)

## Purpose

Stage 2 of the staged delivery (D5), **shipped in pi-studio 0.0.115** (sprint-084/085, 2026-11-13): two panels per daemon that consume pi-studio's read-only observability API (flag `daemonObservability`, advertised by shipped daemons, D30) — a **health panel** (real version, uptime, CPU, memory, event-loop lag, agents by status, connected clients) and a **logs panel** (live tail with level filter and a dropped-lines counter). Stage 1 already shows up/down from `GET /api/health` (200 `{"status":"ok"}`) and connection state; this adds what the protocol lacked (D4). Both panels are read-only subscriptions: no mutation is added to the dashboard's outbound set (D2, D37).

## Public contract

### Wire (consumed; shapes are the shipped fixtures in `packages/protocol/fixtures/external-clients/`, vendored to `crates/protocol/tests/fixtures/pi-studio/`, D22; additive, optional fields)

| Message (fixture file) | Direction | Shape |
|---|---|---|
| server feature flag `daemonObservability` (`status.server_info.json`) | in `server_info.features` | absent ⇒ degraded (below). The existing `daemonStatusRpc` flag is advertised but unimplemented and MUST be ignored (D30). `server_info.version` is now the daemon's real version (e.g. `0.0.115`) |
| `daemon_status_request` / `daemon_status_response` | out / in | response `{ payload: DaemonStatus }`. **Not used by the dashboard**: the subscribe response already carries the first snapshot |
| `daemon_status_subscribe_request` / `daemon_status_subscribe_response` | out / in | per-session; response `{ payload: { ok: true, intervalMs: 5000, status: DaemonStatus } }` — immediate first snapshot and the push cadence |
| `daemon_status_unsubscribe_request` / `daemon_status_unsubscribe_response` | out / in | stops pushes to this session |
| `daemon_status_update` | in (push, every `intervalMs`) | `{ payload: DaemonStatus }` |
| `daemon_logs_subscribe_request` / `daemon_logs_subscribe_response` | out / in | request `{ minLevel, tail }`, `tail ≤ 1000`; response `{ payload: { ok: true, minLevel, effectiveLevel, lines: LogLine[], dropped } }` — `lines` is the backlog, **oldest first**; a repeated subscribe replaces the earlier `minLevel` |
| `daemon_logs_unsubscribe_request` / `daemon_logs_unsubscribe_response` | out / in | — |
| `daemon_log_lines` | in (push, flushed within ~100 ms) | `{ lines: LogLine[], dropped: number }` — `dropped` is incremental; no `requestId`; server-capped at 200 lines/s |

```
DaemonStatus = { version, startedAt, uptimeMs, pid, nodeVersion,
                 memory: { rss, heapUsed, heapTotal }, cpuPercent, eventLoopLagMs,
                 agentsByStatus: { <status>: count },
                 clients: [ { clientId, clientType, appVersion?, connectedAt, remoteAddress?,
                              transport: "direct" | "relay" } ] }
LogLine      = { time (epoch ms), level, msg, fields? }    # from pino NDJSON
```

`clients` includes the requesting session. `effectiveLevel` is the daemon's own level when that is higher than the requested `minLevel` (it may be `silent`). All of these are read RPCs/subscriptions. They are added to the client's outbound allowlist as read requests only (`DaemonStatusSubscribe/Unsubscribe`, `DaemonLogsSubscribe/Unsubscribe`); the mutation set stays exactly `agent_ui_respond_request` and `interrupt_agent` (D2 as revised by D36, D37). Rust types live in `crates/protocol`, lenient: unknown fields tolerated, `transport`/`level` unknown values → `Unknown`, every `DaemonStatus` field optional on the wire and tolerated missing. The dashboard deserialises every fixture `frame` with its own types and honours `volatile`/`direction` (D22).

Access: any authenticated client, no extra gate (D31, upheld in D35).

### Commands (see [app-structure.md](../architecture/app-structure.md) registry)

| id | Params | Effect | Enabled when |
|---|---|---|---|
| `health.open` | `{serverId}` | Opens the health panel for that daemon; sends `daemon_status_subscribe_request` and renders `payload.status` of its response immediately (no separate one-shot request) | daemon connected and flag `daemonObservability` present |
| `health.close` | `{}` | Closes the panel; sends `daemon_status_unsubscribe_request` | a health panel is open |
| `logs.open` | `{serverId, minLevel?}` | Opens the logs panel; sends `daemon_logs_subscribe_request {minLevel, tail: 1000}` (default `minLevel` `info`); the backlog arrives in the response's `lines` | as `health.open` |
| `logs.close` | `{}` | Closes the panel; sends `daemon_logs_unsubscribe_request`; clears the line buffer | a logs panel is open |
| `logs.setMinLevel` | `{minLevel}` | Clears the buffer and **re-subscribes** with the new level (`daemon_logs_subscribe_request {minLevel, tail: 1000}`; the daemon replaces the earlier `minLevel`, no unsubscribe needed) | a logs panel is open |

At most one health panel and one logs panel are open at a time (single drill target each). Opening the panel for another daemon closes the previous one first (unsubscribe, then subscribe).

### State (in `MonitorState.observability`, see [live-state.md](../architecture/live-state.md))

| Field | Contents | Bound |
|---|---|---|
| `health: {serverId, status?: DaemonStatus, intervalMs?, receivedAt, supported: bool} \| null` | latest `DaemonStatus`; `receivedAt` is the local clock at receipt; `intervalMs` from the subscribe response | 1 snapshot |
| `logs: {serverId, minLevel, effectiveLevel, lines: LogLine[], dropped: number, droppedTotal, lastLineAt} \| null` | ring of the latest lines | **5,000 lines** (this spec's proposal; oldest evicted first). Each line's `msg` truncated to 4 KB and `fields` to 8 KB serialised on ingest (hostile input, rule 2) |
| `observabilitySupported: {serverId → bool}` | derived from `server_info.features` | per daemon |

Neither is persisted (D8, D29).

## Behavior & algorithms

### Health panel

```
health.open(serverId):
    if not features(serverId).daemonObservability: fail "daemon too old"       # degraded, D26
    close any open health panel (unsubscribe if subscribed)
    state.health = { serverId, status: None, receivedAt: None }
    send daemon_status_subscribe_request
on daemon_status_subscribe_response { ok: true, intervalMs, status }:
    state.health.status = status; intervalMs = intervalMs; receivedAt = now       # immediate first snapshot
on daemon_status_update (serverId matches open panel):
    state.health.status = payload; receivedAt = now
on connection lost (any reason):
    keep last status, mark "stale since <receivedAt>"; no subscription survives a reconnect
on reconnect (same serverId):
    if panel still open: re-send subscribe (the daemon forgot the subscription)
health.close / panel superseded / daemon removed:
    send daemon_status_unsubscribe_request (if connected); state.health = None
```

The refresh cadence shown and the "stale" threshold derive from `intervalMs` (shipped value 5000), not a hard-coded 5 s.

Display (values formatted by derivation, never in the frontend):
- **Version** (real, from the daemon's `package.json`; badge). Compared with the dashboard-known version only for display; no gating.
- **Uptime**: `uptimeMs` as "3d 4h" / "12m 40s", recomputed between updates from `startedAt` and the local clock (skew accepted, D28); absolute `startedAt` in local time.
- **CPU %** (`cpuPercent`, one decimal), **Memory** (`rss`, `heapUsed`/`heapTotal`, binary units), **Event-loop lag** (`eventLoopLagMs`; ≥ 100 ms warning tint, ≥ 500 ms danger tint — proposal), pid and Node version as meta.
- **Agents by status**: counts per status in canonical order (initializing, idle, running, error, closed) plus any unknown key labelled "unknown"; consistent with, but not derived from, the dashboard's own agent set (the daemon's number is the daemon's truth).
- **Connected clients** table: clientType, `appVersion` (or "—"), clientId (short), connected since (local time), remote address, transport badge (`direct`/`relay`). The row whose `clientId` equals the dashboard's own `hello.clientId` for that connection, or whose `appVersion` starts with `pistudiodashboard/` (D23; the shipped `hello` fixture has `clientType: "cli"`, `appVersion: "pistudiodashboard/0.1.0"`), is marked **"this dashboard"**. Match order: clientId first (authoritative), appVersion prefix second (marks other dashboard instances as "pi-studio dashboard", unmarked as self). The clients list includes the requesting session itself.
- Sorted: self first, then by `connectedAt` ascending. Capped display: 200 clients; beyond that "+N more".

### Logs panel

```
logs.open(serverId, minLevel = "info"):
    require flag daemonObservability, else "daemon too old"
    close open logs panel (unsubscribe)
    state.logs = { serverId, minLevel, effectiveLevel: None, lines: [], dropped: 0 }
    send daemon_logs_subscribe_request { minLevel, tail: 1000 }
on daemon_logs_subscribe_response { ok: true, minLevel, effectiveLevel, lines, dropped }:
    state.logs.effectiveLevel = effectiveLevel
    ingest `lines` (backlog, oldest first) like pushed lines; droppedTotal += dropped
on daemon_log_lines { lines, dropped } (serverId matches):
    for line in lines: truncate msg/fields; push to ring; evict oldest above 5000
    state.logs.droppedTotal += dropped          # incremental server-side drops (cap 200 lines/s)
    state.logs.dropped = dropped (most recent non-zero shown as a "N lines dropped" notice)
logs.setMinLevel(level):
    state.logs.minLevel = level; lines = []; droppedTotal = 0
    send daemon_logs_subscribe_request { minLevel: level, tail: 1000 }     # re-subscribe replaces minLevel
connection lost: keep lines, show "disconnected" banner; on reconnect re-subscribe with the
    same minLevel and tail 1000; mark a "— reconnected —" divider; the new backlog is shown after
    it (duplicates of pre-disconnect lines are possible and accepted)
logs.close: send unsubscribe; discard lines
```

Display: monospace rows (`--pi-font-mono`, `font-size-2xs`): local-time `HH:MM:SS.mmm` (from `time`, epoch ms), level badge (`trace/debug` muted, `info` info, `warn` warning, `error/fatal` danger tint), message, expandable `fields` (pretty JSON, ≤ 8 KB). Auto-scroll to the bottom unless the user scrolled up ("Jump to latest" button); the list is virtualised. Level filter control = the pino levels. When `effectiveLevel` differs from the requested `minLevel` (the daemon's own level is higher), the panel header shows **"daemon logs at <effectiveLevel>"**; with `silent` it states that the daemon emits no logs, so an empty list is not mistaken for a quiet daemon. A **dropped counter** in the panel header ("N lines dropped by daemon") appears whenever `droppedTotal > 0`. A client-side text filter is OUT of scope for stage 2 (deferred).

Server cap and local cap interplay: the daemon caps at 200 lines/s and reports `dropped`; the dashboard additionally bounds memory at 5,000 lines and coalesces view emits to ≤ 1 per 100 ms (D28), so a 200 lines/s stream causes at most 10 view updates/s.

### Degraded mode (D26)

```
daemonObservability absent in server_info.features:
    health.open / logs.open are disabled with reason "daemon too old"
    the daemon's row/diagnostics shows a badge "no observability (daemon too old)"
    the health/logs panels, if reached via a deep route, render the single message
        "This daemon is too old for health and logs — update pi-studio" 
    no request of this family is ever sent; nothing else changes
flag present but request errors (e.g. handler missing, error response):
    panel shows the error text; stays closed-subscription; no retry loop
```

`daemonStatusRpc` present without `daemonObservability` counts as absent (D30).

### Privacy and logging (D29)

- Log lines are **shown only**: never persisted by the dashboard and never written to the dashboard's own log file; the buffer is memory-only and discarded on `logs.close`, daemon removal or app exit.
- The dashboard's own log never records `DaemonStatus.clients[].remoteAddress` or log `fields`/`msg` content; counts and sizes only.
- Daemon log lines may contain sensitive data from the daemon (paths, prompts). They are rendered as text (never as HTML), and the webview receives them only while the panel is open. Notification and tray features never read from this buffer (D32).
- Closing the panel stops the daemon-side stream (unsubscribe), so no log traffic flows while it is closed.

### Performance (D33)

Status sampling on the daemon is < 1% CPU (verified pi-studio-side; see [daemon-status-rpc.md](../../../../pi-studio/swe/features/daemon-status-rpc.md)); the dashboard subscribes only while the panel is open, so a closed panel costs the daemon nothing beyond the existing broadcasts.

## Data & persistence touchpoints

- In-memory `MonitorState.observability` only; `Session` commands `health.*`/`logs.*` are non-recording and mutate no daemon.
- Nothing in the config file. `minLevel` is not persisted (default `info` each open). [TODO(verify): whether the last-used level should persist as a setting.]
- Ingest events: `DaemonStatusSnapshot` (from the subscribe response), `DaemonStatusUpdate`, `DaemonLogsBacklog` (from the subscribe response), `DaemonLogLines`, plus subscription lifecycle events from the client on (re)connect.

## Error handling & edge cases

| Condition | Expected behavior |
|---|---|
| Flag `daemonObservability` absent | Disabled commands; "daemon too old" message; no request sent (D26) |
| Subscribe error response (`ok: false` or `rpc_error`) | Panel shows the error; no automatic retry |
| Daemon disconnects while panel open | Keep last data, mark stale ("stale since <time>" for health; "disconnected" banner for logs); re-subscribe on reconnect (D25) |
| Daemon restarted (same `serverId`) | New subscription on reconnect; `uptimeMs` resets; logs show a divider |
| Daemon replaced (new `serverId` at the address) | Panels for the old `serverId` close (D19) |
| Daemon entry removed/disconnected by the user | Panel closes, unsubscribe sent if still connected |
| Log burst > 200 lines/s | Daemon drops and reports `dropped`; header counter shows total; dashboard stays within 5,000-line ring |
| Oversized `msg`/`fields` | Truncated on ingest with an ellipsis marker; counted in diagnostics |
| `tail` request > 1000 | Never sent; the client clamps to 1000 |
| Malformed push (bad JSON, wrong shape) | Dropped and counted in diagnostics; connection kept (D28) |
| Frame > 16 MiB | Connection closed + reconnect with error (D28) |
| `clients` missing or empty | "No client data" row; panel still renders other fields |
| Unknown level/transport/status value | Rendered as "unknown" badge; never fatal (D26) |
| Own client missing from `clients` (e.g. relay or daemon quirk) | No "this dashboard" mark; no error |
| Clock skew between daemon and dashboard | Absolute times from the daemon rendered in local time zone; relative uptime recomputed with the local clock, skew accepted (D28) |
| Two dashboards on one daemon | Each sees the other as "pi-studio dashboard" in the clients list (appVersion) |

## Dependencies on other specs

- [protocol-client.md](../architecture/protocol-client.md) — wire types, allowlist (read variants added), reconnect/re-subscribe, size caps.
- [live-state.md](../architecture/live-state.md) — `MonitorState.observability` and caps.
- [app-structure.md](../architecture/app-structure.md) — command registry rows, `send_to_daemon`, view coalescing.
- [design-system.md](../architecture/design-system.md) — badges, mono rows, banner and status-bar recipes.
- [daemon-connections.md](daemon-connections.md) — connection lifecycle, feature flags, diagnostics panel placement.
- pi-studio [daemon-status-rpc.md](../../../../pi-studio/swe/features/daemon-status-rpc.md), [daemon-log-streaming.md](../../../../pi-studio/swe/features/daemon-log-streaming.md), [external-client-fixtures.md](../../../../pi-studio/swe/architecture/external-client-fixtures.md) — the producer side of this contract.

## Acceptance criteria

- [ ] Given a stage 2 daemon, when `health.open` runs, then only `daemon_status_subscribe_request` is sent, the panel renders the real version, uptime, CPU %, memory, event-loop lag, agents by status and the connected-clients list from the subscribe response's `status` immediately, and values refresh at the response's `intervalMs` cadence while open. (D1, D30; brief acceptance 15; fixture `daemon_status_subscribe_response.json`)
- [ ] Given the clients list contains the dashboard's own connection, then that row is marked "this dashboard" via its `clientId` and `appVersion: "pistudiodashboard/<version>"` tag, and `clientType` shows `cli`. (D23, D30; brief acceptance 15)
- [ ] Given `health.close`, panel switch, daemon removal, then `daemon_status_unsubscribe_request` is sent and no further updates are applied; after a reconnect with the panel still open, the subscription is re-established without user action. (D30, D25)
- [ ] Given `logs.open` with `minLevel: "info"`, then a `daemon_logs_subscribe_request {minLevel, tail ≤ 1000}` is sent, the response's `lines` backlog appears oldest first, and a new daemon log line at `info` or above is visible within 1 s of emission. (D30; brief acceptance 16; fixtures `daemon_logs_subscribe_response.json`, `daemon_log_lines.json`)
- [ ] Given `logs.setMinLevel`, then a new `daemon_logs_subscribe_request` replaces the level, the buffer clears and only lines at or above the new level arrive; lower levels never appear. (D30; brief acceptance 16)
- [ ] Given a response whose `effectiveLevel` is higher than the requested `minLevel` (including `silent`), then the panel shows "daemon logs at <effectiveLevel>". (D30)
- [ ] Given a stream above 200 lines/s, then the daemon-reported `dropped` values (responses and pushes) are summed and shown as a dropped-lines counter, memory stays ≤ 5,000 lines, and the view updates ≤ 10 times per second. (D28, D30; brief acceptance 16)
- [ ] Given a daemon without `daemonObservability` (including one advertising only `daemonStatusRpc`), then `health.open` and `logs.open` are disabled with "daemon too old", no stage 2 request is sent, and every other feature works. (D26, D30)
- [ ] Given the logs panel was used and closed, then no log line exists in memory, in the dashboard's log file, or on disk, and the daemon stream is unsubscribed. (D29)
- [ ] Given the vendored fixtures for every stage 2 message, then each `frame` deserialises in `crates/protocol` tests and a `DaemonStatus` with missing or unknown fields still renders. (D22, D26)
- [ ] Given a 1-hour run with the health panel open on a stage 2 daemon, then daemon-side sampling stays below 1% CPU. (D33; brief acceptance 17 — measured in pi-studio's test, observed here)
- [ ] Given `health.open`/`logs.open`/`logs.setMinLevel` executed against a recording fake, then the only requests sent are the read requests `daemon_status_subscribe_request`, `daemon_status_unsubscribe_request`, `daemon_logs_subscribe_request`, `daemon_logs_unsubscribe_request` and no mutation type is produced. (D2, D37)

## TODO(verify)

Resolved by the shipped fixtures (pi-studio `47d80e6`): response envelope (`requestId` echoed on subscribe responses, fixture `daemon_logs_subscribe_response.json`); the logs backlog is in the subscribe **response** (`lines`), not a push; `effectiveLevel` and `intervalMs` exist; `status.server_info.version` is real; self-identification via `appVersion: "pistudiodashboard/0.1.0"` in the `hello` fixture.

Resolved in pi-studio code: an invalid subscribe answers `daemon_logs_subscribe_response` with `payload: { ok: false, error: "invalid_level" | "invalid_tail" }` (`observability/daemon-logs-service.ts`), never `rpc_error`; and `clients[].clientId` is the connection's `hello.clientId` (`Session` is built from `hello.data.clientId` in `ws/ws-server.ts` and `daemon/bootstrap.ts`; `observability/daemon-status-service.ts` reports `session.clientId`), so the clientId match is authoritative.

- 5,000-line log ring, 4 KB `msg` / 8 KB `fields` truncation, and 200-client display cap are this spec's proposals (brief fixes only tail ≤ 1000, 200 lines/s, 2,000/500 buffers elsewhere); confirm with the engineer.
- Event-loop-lag tint thresholds (100/500 ms) are proposals.
- Whether `minLevel` should persist as a setting.
- Panel placement in the layout (right panel vs. dedicated view): D15 defines rail/drill-down; health/logs placement must be coordinated with the agent-monitor spec (tentative: right panel replacing the rail, like the drill-down, opened from the daemon chip/diagnostics menu).
