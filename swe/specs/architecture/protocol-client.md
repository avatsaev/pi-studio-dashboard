# Protocol & Client — pi-studio wire types (L0) and daemon connection layer (L4)

> Part of: [overview.md](../overview.md)
> Dependencies: [config-and-secrets.md](config-and-secrets.md) (entries + passwords feed connection tasks), [live-state.md](live-state.md) (consumes `DaemonEvent`), [app-structure.md](app-structure.md) (crate layering, `Services` hook), [../features/daemon-connections.md](../features/daemon-connections.md), pi-studio: [external-client-fixtures](../../../../pi-studio/swe/architecture/external-client-fixtures.md), [daemon-status-rpc](../../../../pi-studio/swe/features/daemon-status-rpc.md), [daemon-log-streaming](../../../../pi-studio/swe/features/daemon-log-streaming.md)
> Source brief: [daemon-monitoring-dashboard](../../grills/daemon-monitoring-dashboard.md) — D2, D3, D12, D16, D17, D22, D23, D25, D26, D28, D29, D30, D36, D37

## Purpose

Two crates give the dashboard its only view of a pi-studio daemon:

- `crates/protocol` (`pistudiodashboard-protocol`, L0) — hand-written, lenient serde types for **only** the messages the dashboard uses, envelope encode/decode, size caps. Pure: no I/O, no workspace dependencies, builds for wasm32 (golden rule 7). (D22)
- `crates/client` (`pistudiodashboard-client`, L4) — owns the tokio runtime and one connection task per configured daemon. It speaks the wire protocol, enforces the outbound allowlist, and emits batched `DaemonEvent`s. It is **not** in the wasm check and the engine does **not** depend on it (events cross into the engine via `feed.ingest`, requests leave via the `Services::send_to_daemon` hook, both wired in the app binary).

The daemon has no role gating (brief "Mutations"), so this layer is the only guard: nothing but the two permitted mutations can be expressed as an outbound message (D2).

## Public contract

### Envelope and frame rules

| Direction | Frame | Shape |
|-----------|-------|-------|
| C→S, first frame | text | `hello` (top-level, not enveloped) |
| S→C, second frame | text | `{type:"status", payload: server_info}` (top-level) |
| both, after handshake | text | `{type:"session", message:{type, requestId?, …}}` |
| both | text | top-level `ping` / `pong` (JSON, never RFC 6455 ping; the server never pings) |
| S→C | binary | **ignored** (terminal/file streams; no terminals in v1). Counted as `binaryFramesIgnored`. The client MUST NOT send `subscribe_terminal_request` (it resizes the PTY; brief Deferred). |

Inbound caps (D28): max text frame 16 MiB (`MAX_FRAME_BYTES = 16 * 1024 * 1024`); over → close with error `frame_too_large`, go to `backoff`. Malformed JSON, non-object JSON, or an envelope without string `type` → dropped, `malformed` counter +1, connection kept. Preview truncation at decode time (D28, D36), on UTF-8 boundaries, with a `truncated: true` marker: in `agent_ui_request.payload` every string field ≤ 2048 bytes and any option list ≤ 100 entries (limits are decisions of this spec). The full payload is dropped, never stored or logged (D29).

### Handshake messages

| Type | Dir | Key fields | Optional / notes |
|------|-----|-----------|------------------|
| `hello` | C→S | `type:"hello"`, `clientId` (string, random UUID generated per process and per entry; not persisted), `clientType:"cli"` (D23), `protocolVersion: 1` | `appVersion: "pistudiodashboard/<version>"` (D23), `capabilities: {}` (the dashboard advertises no `CLIENT_CAPS`; it renders nothing that needs them). Verified: `helloSchema` in `packages/protocol/src/messages.ts`; `clientTypeSchema` is a strict enum `mobile|browser|cli|mcp`. |
| `status` / `server_info` | S→C | `payload.status:"server_info"`, `payload.serverId` (string, required), `payload.capabilities` (map), `payload.features` (map) | `hostname?`, `version?` (the daemon's real version, e.g. `"0.0.115"`; no longer hard-coded `"1.0.0"`), `homeDir?`. `features` values are `unknown` → only `true` counts as "present". Fixture: `status.server_info.json`. |

### Requests the dashboard sends (outbound allowlist, D2)

`OutboundRequest` is a **closed enum** in L0 (`protocol::outbound`). Only variants below exist; there is no generic "send raw JSON" API anywhere in L0/L4 (the connection task accepts `OutboundRequest` and nothing else; the encoder is private to the crate).

| Variant | Wire `type` | Class | Fields | Expected reply |
|---------|-------------|-------|--------|----------------|
| `Ping` | `ping` (top-level) | liveness | `requestId`, `clientSentAt` (epoch ms) | `pong` |
| `ListAgents` | `list_agents_request` | read | `requestId` | `list_agents_response` |
| `ListUi{agentId?}` | `agent_ui_list_request` | read | `requestId`, `agentId?` | `agent_ui_list_response` |
| `SessionStats{agentId}` | `agent_session_stats_request` | read | `requestId`, `agentId` | `agent_session_stats_response` |
| `FetchTimeline{agentId}` (one logical load, D39) / `FetchTimelinePage{agentId, cursor?, direction, limit}` (the client's own paging step) | `fetch_agent_timeline_request` | read | `requestId`, `agentId`, `direction` (`before`\|`after`), `limit`; `cursor` (string) only when present. `FetchTimeline` encodes the first page: `before`, `limit` 200, no cursor | `fetch_agent_timeline_response` |
| `DaemonStatus` | `daemon_status_request` | read (stage 2) | `requestId` | `daemon_status_response` |
| `DaemonStatusSubscribe` / `Unsubscribe` | `daemon_status_subscribe_request` / `daemon_status_unsubscribe_request` | read (stage 2, per-session) | `requestId` | `daemon_status_subscribe_response` `{payload:{ok:true, intervalMs:5000, status: DaemonStatus}}` — the response already carries the first snapshot, so no separate one-shot `DaemonStatus` is needed — then push `daemon_status_update {payload: DaemonStatus}` every `intervalMs`; unsubscribe → `daemon_status_unsubscribe_response` |
| `DaemonLogsSubscribe{minLevel, tail}` / `Unsubscribe` | `daemon_logs_subscribe_request` / `daemon_logs_unsubscribe_request` | read (stage 2) | `minLevel`, `tail` ≤ 1000 (clamped client-side too), `requestId` | `daemon_logs_subscribe_response` `{payload:{ok:true, minLevel, effectiveLevel, lines[] (backlog, oldest first), dropped}}` — the backlog arrives in the RESPONSE — then push `daemon_log_lines {lines, dropped}`. Re-subscribing replaces `minLevel` (this is how `logs.setMinLevel` is implemented). |
| `RespondUi{uiRequestId, response}` | `agent_ui_respond_request` | **mutation** (answers Pi dialogs `confirm`/`select`, D36) | `requestId`, `uiRequestId` (daemon-minted `agent_ui_request.requestId`), `response` = `{confirmed: bool}` (confirm) or `{value: string}` (select); fixtures `agent_ui_respond_request.confirm.json` / `.select.json` | `agent_ui_respond_response` `{payload:{ok:true}}` or `{payload:{ok:false, error:"not_found"}}` (id unknown or already resolved; fixtures `agent_ui_respond_response.ok.json` / `.not_found.json`). Other `error` strings are kept as open strings. |
| `InterruptAgent{agentId}` | `interrupt_agent` | **mutation 2** | `requestId`, `agentId` (fixture `interrupt_agent.json`) | `interrupt_response {agentId, ok, requestId}` (fixture `interrupt_response.json`) |

Every outbound request is wrapped `{type:"session", message:{…}}` except `Ping` and `hello`. `OutboundRequest::class()` returns `Read | Mutation`; `Mutation` is returned exactly for the variants whose `wire_type` is in `MUTATION_ALLOWLIST` (next section).

### Mutation allowlist (D2, D36)

L0 holds one const table, `MUTATION_ALLOWLIST = { agent_ui_respond_request, interrupt_agent }` (exactly two entries, D2 as revised by D36 and D37). `OutboundRequest::class()` is `Mutation` iff `wire_type ∈ MUTATION_ALLOWLIST`, else `Read`. Everything (encoder, `Services::send_to_daemon`, tests) refers to this table; no other mutating type can be expressed. The tool-permission family was retired in pi-studio (D37) and has no outbound or inbound representation here.

### Messages the dashboard consumes

All decode leniently (see rules below). "req" = required for the dashboard to act on the message; a message lacking a required field is dropped and counted `malformed`.

| Wire `type` | Where | Required fields | Optional fields | Notes |
|-------------|-------|-----------------|-----------------|-------|
| `pong` | top-level | `requestId`, `serverReceivedAt`, `serverSentAt` | `clientSentAt` | All timestamps are epoch ms numbers in the fixture (`pong.json`); decoding still tolerates `number \| ISO string` (`wireTimestampSchema`) → `WireTime`, normalised to epoch ms. |
| `rpc_error` | session | `requestId` | `code`, `message`, `error` | Fixture `rpc_error.json`: `{requestId, code:"unknown_message_type", message}`; other codes seen: `handler_error`. Fails the pending request with `RpcError{code, message}`. |
| `list_agents_response` | session | `requestId`, `agents[]` | per agent: `agentId` (req), `status`, `title`, `cwd`, `labels{}`, `lastActivity` (epoch ms), `provider`, `model`, `modelProvider` (absent when the daemon has none, e.g. the mock), `thinkingLevel` | **Flat** (no `payload` wrapper); fixture `list_agents_response.json`. Lists only NON-archived agents (`AgentManager.list()` filters `archivedAt`): archived agents never arrive via the list; an `agent_archived` push removes the card. |
| `agent_update` | session push | `agentId` | `status`, `title` (string\|null), `labels{}` (partial: absent fields = unchanged) | Broadcast to all clients; no subscribe step. **The status source** together with `list_agents_response` (fixtures `agent_update.json`, `agent_update.model.json`). |
| `agent_status` | session push | `agentId`, `status` | | **Never emitted** by any daemon code path (schema-only fixture exception, `agent_status.json`). Decoded for tolerance only; status comes from `agent_update.status` and the list. |
| `agent_archived` | session push | `agentId` | `archivedAt` (ISO-8601 string) | Hides the card (D15). |
| `agent_deleted` | session push | `agentId` | | |
| `agent_stream` | session push | `agentId`, `event` | `seq` (absent for ephemeral events), `timestamp` (ISO-8601 string) | Shape `{agentId, seq, timestamp, event:{kind, …}}`; the discriminator is `event.kind`. Kinds with fixtures: `user_message`, `assistant_message` (`text`, `final`), `reasoning`, `tool_call` (`callId`, `status`, `tool:{kind, command\|path…}`; a partial variant exists), `turn_started`, `turn_completed`, `turn_failed`, `turn_canceled`, `error`, `compaction` (`agent_stream.*.json`). Unknown kinds → `StreamEvent::Unknown{kind}` (counted). Partial `tool_call`s and deltas carry no `seq`. |
| `agent_timeline_reset` | session push | `agentId` | `reason` (open string; shipped value `"fork"`) | Not a stream event; fixture `agent_timeline_reset.json`. |
| `agent_ui_request` | session push | `requestId`, `agentId`, `method` (`select`/`confirm`/`input`/`editor`/`notify`/`setStatus`/other → unknown values kept as string), `expectsResponse`, `payload{}`, `createdAt` (epoch ms) | `surfaceKey`, `removed`, `timeoutMs` | The real "needs you" channel (D36): `confirm` (payload `{title, message}`) and `select` (payload `{title, options: string[]}`) with `expectsResponse` are answerable via `RespondUi`; `input`/`editor` hand off to the web UI; `notify`/`setStatus` are non-dialog surfaces. `setStatus.removed` (`removed:true`, empty payload) = surface-clearing, not an item. Fixtures `agent_ui_request.*.json`. |
| `agent_ui_resolved` | session push | `requestId`, `agentId`, `reason` (open string) | | Daemon emits `answered`, `timeout`, `aborted`; `cancelled` is schema-only (fixture exception) and tolerated. Fixtures `agent_ui_resolved.*.json`. |
| `agent_ui_list_response` | session | `requestId`, `payload.ok`, `payload.pending[]` | `payload.surfaces[]` (`{agentId, method, surfaceKey, payload, updatedAt}`), `payload.error` | `pending[]` holds `agent_ui_request`-shaped dialogs and is the only part used (D7); `surfaces[]` are non-dialog retained surfaces, never attention items. |
| `agent_session_stats_response` | session | `requestId`, `payload.ok`? | `payload.sessionId`, `payload.totalMessages`, `payload.tokens{total, input?, output?, cacheRead?, cacheWrite?}`, `payload.cost`, `payload.contextUsage{tokens?,contextWindow?,percent?}`, `payload.model`, `payload.error` | Fixture `agent_session_stats_response.json` has `tokens:{total}` only and no `cost` (mock): every field beyond `tokens.total` is optional. No live session ⇒ usage "—" (D16). |
| `fetch_agent_timeline_response` | session | `requestId`, `items[]`, `seqStart`, `seqEnd`, `hasNewer` | `agentId`, `startCursor`, `endCursor`, `collapsed`, `sourceSeqRanges[]` | Fixture shape: `items[]` = `{kind, sourceSeq, timestamp (ISO), event}` with `event` = a stream event (`{kind:"user_message", …}`); `kind` is a projection label (fixture: `"other"`) → `TimelineItem` is lenient, `Unknown` for unrecognised kinds. Cursors are opaque strings (`"0"`, `"27"`). |
| `daemon_status_response`, `daemon_status_update` | session | `payload: DaemonStatus` | all inside lenient | Stage 2. `DaemonStatus = {version, startedAt, uptimeMs, pid, nodeVersion, memory{rss,heapUsed,heapTotal}, cpuPercent, eventLoopLagMs, agentsByStatus, clients[{clientId, clientType, appVersion?, connectedAt, remoteAddress?, transport}]}`; `clients` includes the requesting session. Also arrives inside `daemon_status_subscribe_response.payload.status`. |
| `daemon_log_lines` | session push | `lines[]` (`{time (epoch ms), level, msg}`) | `fields`, `dropped` | Stage 2. Same `LogLine` shape as the backlog in `daemon_logs_subscribe_response`. `effectiveLevel` there is the daemon's own level when higher than requested (possibly `"silent"`): the UI shows "daemon logs at <level>". |

Every other `type` (including `terminals_update`, `workspace_update`, `agent_todos_update`, orchestration, etc.) is decoded as `Inbound::Unknown{type}` — ignored, counted per type in `unknownTypes` (bounded map, ≤ 64 distinct keys, then `other`). (D26)

### Feature flags → capability set (D26)

`ServerCaps` is derived from `server_info.features` (only `true` values count):

| Capability | Source flag | Missing ⇒ |
|------------|-------------|-----------|
| `daemon_observability` | `daemonObservability` (stage 2; advertised by shipped daemons) | Health & logs panels show "update daemon" state; the existing `daemonStatusRpc` flag is **ignored** (advertised but unimplemented, D30). |
| `forkTimelineSync` etc. | other flags | ignored |

`DegradedReason` = set of `{no_observability}` (D26, D36): the only degraded state is a daemon lacking `daemonObservability` (stage 2), shown by the health/logs panels; unknown message types / enum values are counted in diagnostics, never banners.

### Events emitted by L4

`DaemonEvent { entryId, serverId?: String, connEpoch: u64, seq: u64 (per-connection monotonic), at: epoch_ms, kind }` where `connEpoch` is a per-entry counter incremented when each new connection attempt starts (first event of an attempt is its `ConnState{connecting|probing}`; D38), and `kind` is one of:

| Kind | Payload |
|------|---------|
| `ConnState` | `state` (state machine below), `since`, `lastError?: ConnError`, `nextRetryInMs?` |
| `Hello` | `ServerInfo{serverId, hostname?, version?, homeDir?}`, `ServerCaps`, `DegradedReason set`, `tlsInUse`, `isLoopback` |
| `Health` | `{reachable: bool, at}` from `GET /api/health` |
| `Snapshot` | `{agents[], uiPending[]}` — post-reconnect re-list from `list_agents_response` + `agent_ui_list_response.payload.pending`; the reducer REPLACES that daemon's agents and pending UI requests (D25 "full re-list") |
| `AgentUpdate` / `AgentStatus` / `AgentArchived` / `AgentDeleted` | decoded push, fields as in the consumed-messages table |
| `AgentStream` | `{agentId, seq?, timestamp?, event}` (foldable deltas may be coalesced, see backpressure) |
| `TimelineReset` | `{agentId, reason?}` from `agent_timeline_reset` |
| `UiRequest` | decoded `agent_ui_request` (`requestId`, `agentId`, `method`, `expectsResponse`, `payload`, `surfaceKey?`, `removed?`, `timeoutMs?`, `createdAt`) |
| `UiResolved` | decoded `agent_ui_resolved` (`requestId`, `agentId`, `reason`) |
| `DaemonStatus` / `DaemonLogLines` | stage 2 pushes/responses incl. the snapshot/backlog carried in the subscribe responses (D30) |
| `Stats` | `{agentId, result: Usage \| NoSession}` |
| `Timeline` | `{agentId, items, reset: bool, seqEnd}` for the drill-down |
| `Rtt` | `{ms}` |
| `Diagnostics` | counters snapshot (see below), at most 1/s |
| `RequestResult` | `{token, result}` for `RespondUi` / `InterruptAgent` (outcome of the user action; `RespondUi` result = `Ok` \| `NotFound` (`ok:false, error:"not_found"`) \| `Rejected{error}`; `InterruptAgent` result = `ok` bool; both may also be `NotConnected`/`Timeout`/`ConnectionLost`) |

Batching: events are delivered as `Vec<DaemonEvent>` on a bounded `tokio::sync::mpsc` channel; the app binary forwards each batch as one `feed.ingest` command (live-state spec owns the reducer).

## Behavior & algorithms

### Connection state machine

```
States: idle → probing → connecting → handshaking → syncing → live
        live/any → backoff → probing …
        terminal (until user acts): auth_failed (401), host_rejected (403), disabled(idle)

on daemon.connect / app start (entry enabled):     idle → probing
probing:   GET http(s)://host:port/api/health, timeout 3 s (no auth, any Host)
             ok            → connecting
             refused/timeout/DNS → backoff (reason unreachable), emit Health{reachable:false}
connecting: open WebSocket ws(s)://host:port/ (any path), subprotocol "pi-studio.bearer.<pw>" iff a password exists,
            TCP+upgrade timeout 5 s
             101           → handshaking
             HTTP 401      → auth_failed        (NO retry; D25)
             HTTP 403      → host_rejected      (NO retry; D25)
             other/error   → backoff
handshaking: send hello; wait ≤ 5 s for status/server_info (first text frame)
             parse ok     → syncing ; emit Hello (caps, degraded set)
             timeout/invalid/close → backoff
syncing:   run sync-on-connect (below) with each request timeout 10 s
             all required done (agents, UI list)       → live ; emit Snapshot
             any required request fails/times out        → close, backoff
live:      read loop + ping timer + stats timer + stage-2 subscriptions
             socket close / read error / frame cap / 2 missed pongs → backoff
backoff:   delay = min(30 s, 0.5 s * 2^attempt) * (1 ± 0.2 uniform jitter)   (D25)
           attempt resets to 0 once state has been live for ≥ 10 s; counts reconnects in diagnostics
             timer fires → probing
daemon.disconnect: close with code 1000, → idle, cancel timers.   daemon.connect on auth_failed/host_rejected: → probing (user supplied a new password / fixed config).
daemon.edit / daemon.setPassword: tear down and restart at probing.
```

The state is emitted on every transition. The mapping error → state uses the HTTP status of the upgrade response. A WS client library that hides the status MUST be wrapped so the 401/403 distinction survives (candidate selection criterion below).

A restart with the same `serverId` is a silent rebuild: the `Snapshot` replaces state without user-visible notices (D25); a different `serverId` at the same entry is surfaced as identity change (D19; reducer + [daemon-connections.md](../features/daemon-connections.md)).

### Sync-on-connect sequence

```
function sync(conn):
    # all issued concurrently after Hello, collected before Snapshot
    agents      = request(ListAgents)                         # required
    # (no permission-list step: the permission family was retired in pi-studio, D37)
    ui          = request(ListUi{})                           # required? no: failure → warn, empty list
    # live pushes that arrive meanwhile are buffered (≤ 10_000 events) and applied AFTER Snapshot,
    # so a resolved request is not resurrected by an older list response
    emit Snapshot{agents, uiPending}
    flush buffered pushes as individual push events (UiRequest, UiResolved, AgentStream, …), order preserved
    schedule stats: SessionStats for every agent with status running, and once for idle/error ones (D16)
    if caps.daemon_observability and panels open: subscribe (stage 2)
```

Race rule: a push `agent_ui_resolved` buffered during sync removes the matching item from `uiPending`; the reducer processes `Snapshot` then buffered events in order, so a resolved dialog is not resurrected. Sync-on-connect is exactly: `list_agents_request` → `agent_ui_list_request` → stats (D16, D36, D37).

### Request/response correlation

```
pending: map requestId → {kind, deadline, oneshot sender}
newRequestId(): "<entryId-short>-<monotonic counter>"       (string; unique per connection)
send(req): register pending (timeout 10 s; stats 15 s; timeline 15 s), write frame
on session message with requestId in pending: complete with typed response
on rpc_error with requestId: complete with Err(RpcError)
on timeout: complete Err(Timeout); the late response, if any, is dropped and counted `lateResponses`
on connection loss: fail all pending with Err(ConnectionLost)
```

Messages with a `requestId` that is not pending (the daemon echoes requestId only for replies) are treated as pushes if their type is a known push, otherwise as unknown. Pending table is bounded (≤ 256); beyond it new non-mutation requests fail fast with `Busy`.

### Mutations (D2, D27)

`RespondUi`/`InterruptAgent` are only issued through `Services::send_to_daemon(serverId, OutboundRequest)`. The client resolves `serverId` → live connection; no connection or not `live` → `Err(NotConnected)` with no queueing (mutations are never replayed after reconnect; D27). The result becomes a `RequestResult` event. `interrupt_response.ok == false` and `agent_ui_respond_response.ok == false` (incl. `error:"not_found"`) are domain results, not errors (the reducer decides messaging per D27). Note (D36): interrupt does not clear a daemon's pending UI requests; they stay pending until resolved.

### Stats polling (D16, D39)

The **engine decides when** to fetch stats (its `FetchStats` effects: on `Snapshot`, on `turn_completed`/`turn_failed`, for a new agent, and every 10 s for running agents from the owner loop's tick — see [live-state.md](live-state.md)); the client never schedules stats on its own. The client **executes** them:

```
on OutboundRequest SessionStats(agent):
    if a request for agent is queued or in flight: drop (dedupe per agentId)
    else queue; ≤ 4 in-flight stats requests per daemon
on response: payload.ok == false → Stats{NoSession}; else Stats{Usage}; never retried by the client
```

### Drill-down timeline fetch (D6, D17, D27, D39)

The **engine decides when** to (re)load the drilled agent's timeline (`FetchTimeline` effect on `agent.select`, on `agent_timeline_reset` for the target, and on `Snapshot` after a reconnect while drilled in). The client **performs the paging** for each such request and never re-opens on its own:

```
on OutboundRequest FetchTimeline(agentId):          # one logical request from the engine
    cancel any paging still running for a previous FetchTimeline of this daemon
    page = request(fetch_agent_timeline_request{agentId, cursor: null, direction: "before", limit: 200})
    emit Timeline{agentId, items, reset:true, seqEnd: page.seqEnd}
    while page.hasNewer and fetched < 2,000:        # gap protection
        page = request(…{cursor: page.endCursor, direction: "after", limit: 200})
        emit Timeline{agentId, items, reset:false, seqEnd: page.seqEnd}
    live agent_stream events already flow as AgentStream; the reducer drops any with seq <= seqHigh
close: no request; the reducer drops the buffer
```

Total items kept by the reducer ≤ 2,000 (D17); the client only requests ≤ 200 per page and stops paging once 2,000 items were fetched. Ephemeral events without `seq` are never used for dedupe.

### Ping/pong RTT (D29)

```
every 15 s while live: send Ping{requestId, clientSentAt: now}
on pong: rtt = now - clientSentAt (local clock only; daemon timestamps not used) → Rtt event; keep last + EWMA
2 consecutive unanswered pings (30 s) → treat as dead connection, close, backoff
```

### Backpressure on the dashboard side (D28, D17)

The daemon cannot slow down (no backpressure; brief "Replay / backpressure"), so the client sheds load itself:

- Read loop never awaits the consumer: decoded events go through a coalescing stage first. Consecutive `agent_stream` deltas for the same agent that carry no state change beyond text/output are folded into a single "activity" event per 50 ms window **unless** the agent is drilled-in (it needs every item). `tool_call` start/complete, `turn_*`, and UI events are never folded.
- Outbound channel to the app: bounded `mpsc` (capacity 1,024 batches). Batch builder flushes on 50 ms tick or 256 events. If the channel is full, the connection task drops *foldable* events first, counts `droppedEvents`, then — if still full — blocks reading (TCP backpressure) rather than dropping attention-class events (UI/status/turn_failed).
- The view-side 100 ms coalescing (D28) is the app binary's job, downstream of this.
- Per-daemon memory: no unbounded queues anywhere; sync buffering limit above; stats queue ≤ 1/agent.

### Diagnostics counters (D29)

Per connection, reset on process start (not on reconnect except where noted): `state`, `lastError{kind,message,at}`, `reconnectCount`, `messagesTotal`, `messagesPerSecond` (1 s window), `unknownTypes{type→n}`, `unknownEnumValues`, `malformed`, `binaryFramesIgnored`, `framesTooLarge`, `droppedEvents`, `lateResponses`, `rttMs{last, ewma}`, `connectedSince?`. Emitted as `Diagnostics` ≤ 1/s and only consumed by the diagnostics panel. Logs of the client crate: connection transitions and counters only — never passwords, bearer strings, subprotocol values, or `action` payloads (D29); the URL is logged without credentials, and the subprotocol header is redacted by construction (see config-and-secrets).

### Lenient decoding rules (D22, D26)

1. Unknown JSON fields are tolerated everywhere (no `deny_unknown_fields`).
2. Every enum that crosses the wire has an `Unknown` fallback (`AgentStatus::Unknown`, `StreamEvent::Unknown`, `Inbound::Unknown`); an unknown value increments `unknownEnumValues` and renders as "unknown" (D26).
3. Fields marked optional above are `Option`; a wrong *type* for an optional field decodes as `None` (lenient field deserialisers), not as an error, so one bad field cannot drop a whole agent list. Wrong type for a *required* field drops the message (`malformed`).
4. Timestamps: `number | string` → epoch ms; unparsable ISO → `None`.
5. `features`/`capabilities` map values of any JSON type; only literal `true` enables a capability.
6. Strings that go to the UI are length-capped at decode (title 256 chars, UI payload strings 2 KB, error text 1 KB) to bound memory (D28).
7. L0 round-trip: types are `Serialize + Deserialize` so fixtures and tests can round-trip, but only `OutboundRequest` has an encoder.

### Outbound allowlist enforcement (D2)

Enforced at three levels:

1. **Type level**: `OutboundRequest` is a closed enum; the only encode function takes `&OutboundRequest`; `OutboundRequest::wire_type()` is exhaustive (compile error when adding a variant without classifying it).
2. **Visibility**: the raw sink of the WebSocket is private to the connection task; the `Services` hook signature accepts only `OutboundRequest`.
3. **Test**: `tests/outbound_allowlist.rs` (L0) iterates every variant (via an `all_variants()` list checked by an exhaustive `match`), encodes it, and asserts `wire_type` is in the read set or in `MUTATION_ALLOWLIST`, and that `class() == Mutation` exactly for the two members of `MUTATION_ALLOWLIST` (`agent_ui_respond_request`, `interrupt_agent`). A second test greps the `protocol` source for string literals of known mutating pi-studio types (e.g. `delete_agent`, `archive_agent`, `send_agent_prompt`, `kill_terminal`, `create_agent_request`) and fails if any appears outside tests/fixtures. A third test (L4) runs a fake daemon, drives every public client method, and asserts the set of `type`s received is a subset of the allowlist.

### WebSocket / TLS crate candidates (rule 9: permissive licences only)

| Concern | Candidates | Selection criteria |
|---------|-----------|--------------------|
| WebSocket client | `tokio-tungstenite` (MIT), `fastwebsockets` (Apache-2.0) | Must: custom `Sec-WebSocket-Protocol`, access to the HTTP status of a rejected upgrade (401/403 distinction — in tungstenite this arrives as `Error::Http(Response)`), `max_message_size` / `max_frame_size` settable to 16 MiB, text vs binary frames. |
| TLS (`wss://`, `https` probe) | `rustls` + `tokio-rustls` (Apache-2.0/MIT/ISC), roots from `rustls-native-certs` or `webpki-roots` (MPL-2.0 data — check) | Avoid OpenSSL linkage (distribution on Linux + macOS, D34). |
| HTTP health probe | `hyper`/`hyper-util` (MIT) minimal client, or `ureq`-style blocking in `spawn_blocking`, or `reqwest` (MIT/Apache-2.0) | Small dependency surface; must apply a timeout and send no credentials. |
| Runtime/misc | `tokio` (MIT), `serde`/`serde_json` (MIT/Apache-2.0), `url` | |

TLS decisions to settle in implementation: self-signed certificates on LAN daemons are rejected by default in v1 (no "trust anyway" option; TODO(verify) whether the user wants one — outside brief).

## Data & persistence touchpoints

- Nothing persisted by these crates (D8). Inputs: `DaemonEntry` + optional password supplied by the app binary (see [config-and-secrets.md](config-and-secrets.md)); output: `DaemonEvent` stream.
- Fixtures (pi-studio `packages/protocol/fixtures/external-clients/`, also published in the `@av-pi-studio/protocol` npm package under `fixtures/external-clients/`; contract: [external-client-fixtures](../../../../pi-studio/swe/architecture/external-client-fixtures.md), directory `README.md`): `manifest.json` = `{formatVersion: 1, messages:[{file, type, direction, response?, exception?}]}`; each `<type>[.<variant>].json` = `{type, direction (server_to_client\|client_to_server\|http), description, since, stage, requiresFeature, volatile: [JSON Pointers], frame}` where `frame` is the complete frame on the socket (`{type:"session", message:{…}}`, bare `status`/`ping`/`pong`; for `health` `{status, contentType, body}`). Arrays hold one representative element. `CHANGELOG.md` + a `formatVersion` bump announce breaking changes. The dashboard vendors the directory **verbatim** to `crates/protocol/tests/fixtures/pi-studio/`, with a `SOURCE.json` recording `{piStudioCommit, piStudioVersion, copiedAt}`. The L0 test `fixtures_deserialize.rs` iterates `manifest.json` and, for every entry, deserializes `frame` with the dashboard's own types and asserts: no `malformed`, no `Unknown` for consumed types, key required fields present. It honours `direction` (only `server_to_client` frames go through the inbound decoder; `client_to_server` frames must equal what `OutboundRequest` encodes, comparing everything except `volatile` pointers, whose presence and JSON type are still asserted), `volatile` (values not compared) and `exception` (`agent_status`, `agent_ui_resolved.cancelled`: decoded for tolerance, never expected live). A second check fails when the manifest lists a consumed type the dashboard does not decode, or a consumed type has no fixture; a `formatVersion` higher than the one the tests were written against fails loudly. (D22, D37)
- A hand-written "degraded" fixture set (`server_info` without `daemonObservability`; unknown stream event kind; unknown status; unknown `agent_ui_request.method`) lives beside the vendored ones in `tests/fixtures/local/` and is not overwritten by a re-vendor.
- Fake daemon for L4 tests: an in-process tokio WebSocket server in `crates/client/tests/support/` replaying fixtures; scriptable to return 401/403, drop the connection, send oversize/malformed frames, and delay `pong`.

## Error handling & edge cases

| Condition | Expected behavior |
|-----------|-------------------|
| Health probe OK but WS refused with 401 | `auth_failed`; no retry; event `ConnError{kind: auth_failed}`; password prompt is raised by the app (D25). |
| 403 on upgrade | `host_rejected`; message carries the host that was used so the UI can say "add `<host>` to `PI_STUDIO_HOSTNAMES` or connect by IP" (D25). |
| Health probe fails but WS would work (health path moved) | Probe failure only affects `Health` event and starts the attempt anyway after one failed probe: a probe failure never blocks a WS attempt more than once per backoff cycle. TODO(verify) acceptable. |
| Frame > 16 MiB | Close, `framesTooLarge`+1, `lastError frame_too_large`, backoff (D28). |
| Malformed JSON | Dropped, `malformed`+1, connection kept (D28). |
| Binary frame | Ignored, counted. |
| First frame is not `status/server_info` | `handshaking` fails → backoff, error `bad_handshake`. |
| `server_info.serverId` empty/missing | handshake fails (`bad_handshake`); cannot key the daemon (D19). |
| Pending mutation when the connection drops | `RequestResult{Err(ConnectionLost)}`; never auto-retried (D27). |
| `agent_ui_resolved` for unknown requestId | Forwarded; reducer ignores. |
| Daemon sends stage-2 push without subscription | Ignored as unknown if not decodable; decoded and forwarded otherwise. |
| `hello` rejected (socket closed right after hello) | → backoff; after 3 consecutive handshake failures the diagnostics `lastError` says `handshake_rejected` and backoff stays at the cap. TODO(verify) daemon behaviour for an unsupported `protocolVersion`. |
| Password contains non-token characters | The subprotocol must be a valid HTTP token. TODO(verify) how pi-studio's own client encodes passwords containing disallowed characters (see `packages/client`); until verified, reject such passwords at `daemon.setPassword` with a clear message rather than percent-encoding by guess. |
| System sleep/wake | Missed pongs trigger reconnect; backoff attempt counter reset on wake is not attempted (kept simple). |
| 500 agents × stats | ≤ 4 concurrent per daemon, 10 s cadence only for running agents (D16, D33). |

## Dependencies on other specs

- [config-and-secrets.md](config-and-secrets.md) — `DaemonEntry` fields, keychain password retrieval.
- [live-state.md](live-state.md) — consumes `DaemonEvent`; defines `feed.ingest`, rings, and the reducers that interpret `Snapshot`, `Inbound` ordering and `seq` dedupe.
- [attention-and-interventions.md](../features/attention-and-interventions.md) — user flows around `RespondUi`/`InterruptAgent` results (D27, D36, D37).
- [daemon-connections.md](../features/daemon-connections.md) — user-facing states and messages.
- [app-structure.md](../architecture/app-structure.md) — where the runtime is started, the `Services::send_to_daemon` hook.
- [daemon-health-and-logs.md](../features/daemon-health-and-logs.md) — consumes stage 2 messages.
- pi-studio specs listed in the header — server-side contracts for the new wire names.

## Acceptance criteria

- [ ] Given a fake daemon, when the client connects with a password, then the upgrade request offers subprotocol `pi-studio.bearer.<pw>` and the first frame sent is a `hello` with `clientType:"cli"`, `protocolVersion:1` and `appVersion` matching `pistudiodashboard/<semver>`. (D23)
- [ ] Given every `OutboundRequest` variant, when encoded, then `wire_type` belongs to the read set or to `MUTATION_ALLOWLIST` = exactly {`agent_ui_respond_request`, `interrupt_agent`}, and only those two are `Mutation`; the source-scan test finds no other mutating pi-studio type literals, and no `permission` wire type exists in L0. (D2, D36, D37)
- [ ] Given upgrade responses 401 and 403, when connecting, then the state ends in `auth_failed` / `host_rejected` respectively and no further connection attempt is made without a user action. (D25)
- [ ] Given an unreachable daemon, when retrying, then delays follow 0.5 s ×2 capped at 30 s with ±20% jitter over a simulated clock. (D25)
- [ ] Given a connected daemon, when it sends a 17 MiB frame, then the connection closes with `frame_too_large`, `framesTooLarge` is 1 and the client reconnects with backoff. (D28)
- [ ] Given a frame `{not json`, when received, then `malformed` increments and the connection stays `live`. (D28)
- [ ] Given any daemon, when connecting, then the sent request types are exactly `list_agents_request`, `agent_ui_list_request` and then stats requests, and an `agent_ui_request` with `method:"confirm"`/`"select"` and one with an unknown `method` both decode without error. (D36, D26)
- [ ] Given a `server_info` advertising only the legacy `daemonStatusRpc`, then `daemon_observability` is false. (D30)
- [ ] Given a message with an unknown `type` and a status with an unknown enum value, then each is counted, nothing panics, and the status decodes as `Unknown`. (D26)
- [ ] Given a sync in progress, when `agent_ui_resolved` arrives before the `agent_ui_list_response`, then the final state contains no such pending request. (D27, D36)
- [ ] Given a pending request that gets no answer, when 10 s elapse, then it fails with `Timeout` and a later response increments `lateResponses`. 
- [ ] Given a `SessionStats` request for an agent that already has one queued or in flight, then no second request is sent; given a response with `ok:false`, then `Stats{NoSession}` is emitted; the client sends no stats request it was not asked for. (D16, D39)
- [ ] Given a `FetchTimeline` request whose first page has `hasNewer:true`, then the client follows with `direction:"after"` pages from `endCursor` until `hasNewer:false` or 2,000 items, emitting one `Timeline{reset:true}` then `Timeline{reset:false}` events; a new `FetchTimeline` cancels the previous paging. (D17, D27, D39)
- [ ] Given a stalled event consumer, when the daemon streams 50 k deltas, then memory stays bounded, attention-class events are not dropped, and `droppedEvents` counts only foldable ones. (D17, D28)
- [ ] Given every entry of the vendored `manifest.json`, then its `frame` deserializes with the dashboard's types (honouring `direction`, `volatile`, `exception`), the manifest and the directory list the same files, and `SOURCE.json` holds a pi-studio commit hash and version. (D22, D37)
- [ ] Given an `agent_ui_respond_response` with `{ok:false, error:"not_found"}`, then `RequestResult` is `NotFound`, not an error. (D37)
- [ ] Given a `daemon_status_subscribe_response` and a `daemon_logs_subscribe_response`, then the status snapshot and the log backlog are emitted as `DaemonStatus` / `DaemonLogLines` events without any extra request, and `effectiveLevel` is exposed. (D30)
- [ ] Given `cargo xtask wasm`, then `pistudiodashboard-protocol` builds for wasm32 and `pistudiodashboard-client` is excluded. (golden rule 7)
- [ ] Given a connection with a password, when logs and `Diagnostics` events are captured during connect and failure, then neither contains the password or any dialog payload text. (D29)

## TODO(verify)

Resolved in pi-studio code: `fetch_agent_timeline_request` accepts `direction: "before" | "after"` (`timelineDirectionEnum` in `protocol/src/messages.ts`; `agent/timeline-rpc.ts`); without a cursor `"before"` returns the newest page, and the response shape is the same as the fixtured `"after"` page (`agent/timeline-store.ts`). Only `"after"` has a fixture.

- Whether `interrupt_response` is absent from the protocol Zod union (cosmetic; the fixture pins its frame).
- How pi-studio's own client encodes passwords into the bearer subprotocol (token-character restrictions).
- Daemon behaviour on an unsupported `protocolVersion` or an invalid `clientType`.
- Licences of `tokio-tungstenite`/`fastwebsockets`, `rustls`, `rustls-native-certs`/`webpki-roots` (MPL-2.0 data), `reqwest`/`hyper` (golden rule 9) — resolves the brief's licence TODO for the WebSocket crate.
- Whether self-signed `wss://` certs need a user override (not decided by the brief).
