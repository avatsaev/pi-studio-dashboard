# Architecture — Live State (model, ingest, feed)

> Part of: [overview.md](../overview.md)
> Dependencies: [protocol-client.md](protocol-client.md) (produces `DaemonEvent`), [app-structure.md](app-structure.md) (layers, owner thread, `UiState` vs `MonitorState` split), [agent-monitor.md](../features/agent-monitor.md), [attention-and-interventions.md](../features/attention-and-interventions.md), [notifications-and-tray.md](../features/notifications-and-tray.md), [daemon-health-and-logs.md](../features/daemon-health-and-logs.md) (owns `MonitorState.observability`)
> Source brief: [daemon-monitoring-dashboard.md](../../grills/daemon-monitoring-dashboard.md) (D8, D15–D17, D19, D25–D28, D33, **D36**, **D37**)

## Purpose
Defines the in-memory, live-only state of the dashboard (`MonitorState`, L1 `crates/model`), the pure reducers that fold daemon events into it (L2 `crates/ingest`), the derivations computed from it, and how events enter the engine (`feed.ingest`, L5). Nothing here is persisted (D8); the daemon is the source of truth. All bounds (D17, D28) are enforced by reducers, not callers.

Per D36 and D37, the things that "need the engineer" are **Pi dialogs**: `agent_ui_request` with `expectsResponse: true`. They are first-class items (`uiRequests`) and the only attention-item kind besides failures. The tool-permission channel was retired in pi-studio (D37) and has no state, events or reducers here.

## Public contract

### `MonitorState` (L1, plain serde data; no I/O; wasm-safe)
Matches the split in [app-structure.md](app-structure.md): selection/drill-down target and observability panel state live **here**; `UiState` (shell) holds theme, filters, rail collapsed, dialog, diagnosticsTarget, toasts, focusItem.

| Field | Shape | Notes |
|-------|-------|-------|
| `daemons` | ordered map `entryId → DaemonSlice` | config order; keyed by local `entryId` (D19) |
| `DaemonSlice` | `{ entryId, name, conn: ConnState, connEpoch: u64, serverId?, info?: {hostname?, version?, features}, degraded: set<DegradedReason>, duplicateOf?, health?: {reachable, at}, staleSince?, counters: {unknown, malformed, …}, events: ring ≤ 500 }` | `ConnState`, `DegradedReason` (only `no_observability`) are defined by protocol-client. `connEpoch` = newest connection epoch seen for this entry (D38). |
| `agents` | map `(serverId, agentId) → AgentEntry` | D19 |
| `AgentEntry` | `{ serverId, agentId, title?, cwd?, labels, provider?, model?, status: AgentStatus, lastActivity?, archived, activity: ActivitySummary, usage: UsageSnapshot, attention: { pendingDialogs: n, errored: bool }, stale }` | `attention` is a reducer-maintained cache; `status` comes from `list_agents_response` and `agent_update.status` only (`agent_status` is never emitted); `lastActivity` is epoch ms from the list |
| `AgentStatus` | `initializing \| idle \| running \| error \| closed \| Unknown` | unknown wire → `Unknown` (D26) |
| `ActivitySummary` | `{ currentTool?: {callId?, kind, label, status}, lastOutputLine?, turnState: none\|started\|completed\|failed\|canceled, lastErrorMessage? }` | D17: derived only |
| `UsageSnapshot` | `none` \| `{ tokens?, cost?, contextUsage?, takenAt }` | none → "—" (D16) |
| `uiRequests` | map `(serverId, requestId) → UiRequestItem` | **first-class**, D36 |
| `UiRequestItem` | `{ serverId, requestId, agentId, method: string, kind: Confirm\|Select\|Input\|Editor\|Other, answerability: Answerable\|HandOff, title?: ≤ 200 chars, message?: ≤ 500 chars (confirm), options?: string[] (select; ≤ 50 entries, each ≤ 120 chars; more are dropped and flagged `optionsTruncated`), timeoutMs?, createdAt (epoch ms, from wire), expiresAt? (local, derived), raisedAt (local receipt time), inFlight: bool, inFlightResponse? }` | Built from `agent_ui_request.payload` fields `title`, `message`, `options` (pi-studio `AskCard.tsx` reads exactly `title`, `message`, `options: string[]`). `placeholder`/prefill/`editor` text is **never** retained (D29/D32). |
| `drilldown` | `Option<DrillBuffer>` (`drilldown.target = (serverId, agentId)`) | engine commands `agent.select/deselect` set it (app-structure) |
| `DrillBuffer` | `{ target, items: ring ≤ 2000 of DrillItem, seqHigh?, state: loading\|live\|refetching, truncatedAtStart, stale }` | D17 |
| `DrillItem` | `assistant{…} \| reasoning{…} \| tool{callId, kind, label, status, outputTail? (last 12 lines, each ≤ 500 chars)} \| error{message} \| turn{kind} \| system{text}` | |
| `observability` | stage 2 panel state, `minLevel`, subscription flags, status/log buffers | owned by [daemon-health-and-logs.md](../features/daemon-health-and-logs.md) |
| `settings` | notification toggles (`needsInput`, `failures`), `closeToTray`, theme mirror | single source decided in app-structure TODO |
| `EventRecord` (ring entry) | `{ at, kind: dialogRaised\|dialogResolved\|turnFailed\|agentError\|connState\|agentRemoved, agentId?, summary }` | `summary` never contains option text, message text, command or path content (D29, D32) |

### `DaemonEvent` (input to L2; the enum is **owned by protocol-client.md**; this spec consumes it)
`DaemonEvent { entryId, serverId?, connEpoch, seq, at, kind }` (`connEpoch`: per-entry counter the client increments per connection attempt, D38) with kinds (exactly protocol-client.md § "Events emitted by L4"): `ConnState`, `Hello(ServerInfo, ServerCaps, DegradedReason = {no_observability}, tlsInUse, isLoopback)`, `Health`, `Snapshot{agents[], uiPending[]}` (replaces that daemon's agents + pending UI requests), `AgentUpdate`, `AgentStatus` (decoded for tolerance; never emitted by daemons), `AgentArchived`, `AgentDeleted`, `AgentStream{agentId, seq?, timestamp?, event}`, `TimelineReset{agentId, reason?}`, `UiRequest{requestId, agentId, method, expectsResponse, payload, surfaceKey?, removed?, timeoutMs?, createdAt}`, `UiResolved{requestId, agentId, reason}`, `DaemonStatus`, `DaemonLogLines` (stage 2; reducers owned by daemon-health-and-logs), `Stats{agentId, Usage | NoSession}`, `Timeline{agentId, items, reset, seqEnd}`, `Rtt`, `Diagnostics`, `RequestResult{token, result}` (token correlates to a user action: `RespondUi`, `InterruptAgent`).

Timestamps: `createdAt`/`lastActivity` are epoch ms on the wire; `agent_stream.timestamp` and `agent_archived.archivedAt` are ISO-8601 strings, normalised to epoch ms by L0 (protocol-client rule 4).

### Engine surface (L5)
| Item | Contract |
|------|----------|
| `feed.ingest` | Internal command `{ events: DaemonEvent[] }` (app-structure; no menu). Non-recording (no undo; live data is not an edit); one revision bump per batch; returns `Effect`s. A malformed batch is rejected with state untouched. |
| `EngineServices::send_to_daemon(serverId, OutboundRequest)` | Outbound requests (stats poll, timeline fetch, `RespondUi`, `InterruptAgent`, stage 2 subscriptions). Allowlist enforced in L0/L4 (protocol-client `MUTATION_ALLOWLIST` = `agent_ui_respond_request`, `interrupt_agent`). |
| `Effect` (exactly the Effects table in app-structure.md; reducers stay pure, the owner thread executes them) | `Notify{kind, serverId, itemId}` (`kind` ∈ needsInput\|failures; `itemId` = `"ui:<requestId>"`, `"fail:<agentId>"`; title/body are built by the notifier from the item, see notifications spec); `ResolveNotification{serverId, itemId}`; `Toast{kind, text}`; `CloseDrill{reason}`; `FetchStats{serverId, agentId}`; `FetchAgents{serverId}` (re-list after a new agent appears by push; answered by `AgentsListed`); `FetchTimeline{serverId, agentId}` (one logical (re)load; the client does the paging, D39). The engine is the only place that decides *when* stats and timelines are fetched; the client only executes and dedupes (D39). |

### View derivations (read by L6 `view`; pure functions of `MonitorState`, never stored)
`card_order(filters)`, `daemon_status_counts(daemon)`, `usage_totals(filter) → {tokens, cost, covered, of}`, `attention_items()` (defined in attention spec), `ui_request_actions(item)`.

## Behavior & algorithms

### Flow (grow pattern 02, early-decisions "live data")
```
client tasks (tokio, L4) ──Vec<DaemonEvent>──▶ app-binary batcher (≤ 50 ms) ──Request::Command(feed.ingest)──▶ owner-thread channel
         (same channel as Tauri commands / control socket)
owner thread: Session.execute("feed.ingest") → reducers → effects → AppShell view → compare with last → coalesced emit (app binary, ≤ 1/100 ms, D28)
```
- Workers never touch `MonitorState`. Per-daemon event order is preserved (single FIFO per daemon; `seq` monotonic per connection).
- View coalescing ≤ 1 emit / 100 ms is the app binary's job; engine/shell are not time-aware. Commands (clicks) are never delayed more than one window. Budget: 50 ms batch + 100 ms emit + IPC < 500 ms to a visible dialog (D33).
- Reducers take `now` as an input argument (no clock reads in L1/L2; deterministic tests).

### Top-level reducer
```
function apply(state, event) -> (state', effects):
    slice = state.daemons[event.entryId]  (ignore if the entry was removed)
    if event.connEpoch < slice.connEpoch: drop                                 # stale: from an older connection (D38)
    if event.kind is ConnState and event.connEpoch > slice.connEpoch: slice.connEpoch = event.connEpoch
    match event.kind:
      ConnState(s, err):   slice.conn = s; slice.lastError = err
                           if s is disconnected-ish (backoff/auth_failed/host_rejected) and slice.serverId: mark_stale(slice)
                           push_event(slice, connState)
      Hello(info, caps, degraded): resolve_identity(state, entry, info.serverId); slice.info = ..; slice.degraded = degraded
      Health(r): slice.health = r
      Snapshot(s): return rebuild(state, entry, s)
      AgentUpdate / AgentStatus / AgentArchived / AgentDeleted / AgentStream / TimelineReset / UiRequest / UiResolved /
      Stats / Timeline / RequestResult: per-event reducers below
      DaemonStatus / DaemonLogLines: observability reducers (daemon-health-and-logs.md)
      Rtt / Diagnostics: update slice counters (diagnostics panel reads them)
```

### Identity (D19)
```
resolve_identity(state, e, serverId):
    other = connected entry with same serverId, other != e
    if other: slice(e).duplicateOf = other.entryId; drop agents/uiRequests belonging to this slice   # no double counting
    else: slice(e).duplicateOf = none
    if slice(e).serverId = old and old != serverId:
        remove all agents/uiRequests/events of old; close drilldown if its target.serverId == old (notice "daemon replaced")
    slice(e).serverId = serverId
```
First `Hello` processed on the owner thread owns the `serverId`; later entries become duplicates (deterministic by arrival).

### Rebuild-on-connect (atomic replace per daemon; D8, D25)
```
function rebuild(state, e, snap):
    sid = slice(e).serverId
    new_agents    = { (sid,a.id): AgentEntry::from_list_item(a) for a in snap.agents }
    carry-over for keys already present: usage (keep takenAt; shown "as of"), activity only if status == running
    new_ui        = { (sid,u.requestId): UiRequestItem::from_pending(u) for u in snap.uiPending if u.expectsResponse and !u.removed }
    # single assignment per map: (old minus serverId == sid) ∪ new   → no frame with an empty slice
    recompute attention caches for the slice; slice.staleSince = none
    effects += FetchStats for agents with status running or no usage                 # D16
    if drilldown.target.serverId == sid:
        if target in new_agents and not archived: drilldown.state = refetching; effects += FetchTimeline(newest)
        else: effects += CloseDrill("agent gone")
    notification baseline: items present in the snapshot but NOT in the pre-disconnect set count as "new" only if
        state.baselineDone(slice) (see notifications spec)
```
Buffered pushes the client collected during sync arrive as ordinary events **after** the `Snapshot` (protocol-client sync rule), so an `agent_ui_resolved` that raced the list removes the item from the list result (no resurrection).

### Stale marking (D25)
`mark_stale(slice)`: `staleSince ??= now`; every agent of the slice `stale = true` (dim + "stale since"); dialog items stay listed but flagged `stale` and their answer controls are disabled (reason "daemon disconnected"); in-flight flags of the slice are cleared (the answer may or may not have landed; rebuild is authoritative); drill buffer retained read-only, flagged stale. Only `rebuild` clears it. Agents are never dropped on disconnect.

### Per-event reducers
```
AgentUpdate(a, status?, title?, labels?) / AgentStatus(a, status):     # AgentUpdate is partial: absent fields are left untouched; AgentStatus never arrives in practice
    entry = agents[a]; if absent: create placeholder {status ?? idle, title} and effects += FetchStats, FetchAgents   # until AgentsListed (cwd, provider) / next Snapshot
    patch present fields; lastActivity = now on status/title change; if running → not running: clear currentTool
    after_status_change(entry, old, new)
after_status_change(entry, old, new):
    if new == error and old != error: attention.errored = true; push_event(agentError); effects += Notify(failures)   # D9
    if old == error and new != error: attention.errored = false
AgentArchived(a): entry.archived = true; drop its uiRequests; if drilldown.target == a: CloseDrill("archived")  # D27
AgentDeleted(a):  remove entry + its uiRequests; CloseDrill("deleted") if target
AgentStream(a, seq?, event): update_activity(entry, event) (table below); if drilldown.target == a: drill_apply(event, seq)
                             turn_completed/turn_failed → effects += FetchStats
UiRequest(u):
    if u.removed or !u.expectsResponse: ignore (surface/notify methods are not attention items)       # D36
    else upsert uiRequests[(sid,u.requestId)] = from_wire(u)   # derives kind, answerability, expiresAt
         agent.attention.pendingDialogs = count; push_event(dialogRaised)
         if newly inserted: effects += Notify(needsInput, item)
UiResolved(requestId, reason):
    item = uiRequests.remove(key); if absent: no-op
    recount; push_event(dialogResolved)
    if item.inFlight: silent                           # our own answer landed
    else match reason:
         "answered"  → Toast(info, "answered elsewhere"), ResolveNotification(item)
         "timeout"   → Toast(info, "request timed out"), ResolveNotification(item)
         "aborted"   → Toast(info, "request aborted"), ResolveNotification(item)
         "cancelled" (schema-only, tolerated) | other → ResolveNotification(item); Toast(info, "request <reason>")
RequestResult(token, result):
    RespondUi:    ok → keep item until UiResolved (idempotent; item removed immediately for responsiveness); inFlight=false
                  ok:false error "not_found" → remove item, Toast(info, "no longer pending"); other error → inFlight=false, Toast(info, "answer failed: <error code>")
    InterruptAgent: ok:true → clear flag; ok:false → clear flag, Toast(info, "<agent> was not running")
    (interrupt does NOT clear any dialogs: pi-studio extension-ui-rpc.md "Interrupt touches nothing")
Stats(a, Usage|NoSession): usage = {…, takenAt=at} or none
Timeline / TimelineReset: drilldown only
expire_tick(now) (driven by the owner loop at ≤ 1 Hz while any item has expiresAt): items past expiresAt get `expired=true` in derivation (controls disabled); NOT removed — only `agent_ui_resolved{timeout}` removes (daemon decides)
stats_tick(now) (driven by the owner loop every 10 s while any connected agent is running): effects += FetchStats for every running agent of connected daemons   # D16, D39
```
Idempotency: removals tolerate absent keys; insertions upsert by key; duplicate delivery (broadcast + list, or RPC result + `agent_ui_resolved`) yields the same state.

### Dialog classification (`from_wire`)
```
kind = match method: "confirm"→Confirm, "select"→Select, "input"→Input, "editor"→Editor, _→Other
title   = payload.title (string) capped; message = payload.message (string, confirm only) capped
options = payload.options if array of strings else none           # select; non-string entries dropped
answerability = Answerable if kind == Confirm
                or (kind == Select and options.len >= 1)
                else HandOff                                       # input, editor, select without options, unknown methods
expiresAt = createdAt + timeoutMs (converted to local; clock skew accepted, D28), only if timeoutMs present
```
Option text and message are retained only for rendering in the app window (bounded above), never logged, never in `EventRecord.summary`, notifications, or toasts (D29, D32).

### Activity summary derivation from `agent_stream` events
Source: pi-studio `docs/agent-stream-events.md` (11 kinds, append-only; unknown kind → ignore + count, D26).

| `event.kind` | `currentTool` | `lastOutputLine` | `turnState` / other |
|--------------|---------------|------------------|---------------------|
| `turn_started` | cleared | cleared | `started`; `lastErrorMessage` cleared |
| `assistant_message` text (not `final`) | unchanged | last non-empty line of the assistant-text tail buffer | — |
| `assistant_message` `final:true` | unchanged | unchanged; closes buffer | — |
| `reasoning` | unchanged | unchanged (never "output"; drill only) | — |
| `tool_call` status `started`/`running` (partial or not) | set `{callId, kind, label=toolDetailText, status}`; label single line ≤ 120 | partial with non-empty `output`: last non-empty line (≤ 200) | — |
| `tool_call` `completed`/`error` | cleared if `callId` matches | non-empty `output`: last non-empty line | — |
| `tool_call` other status string | treated as running | — | status is a free string |
| `turn_completed` | cleared | unchanged | `completed` |
| `turn_failed{error?}` | cleared | unchanged | `failed`; `lastErrorMessage` ≤ 200; `attention.errored = true`; `Notify(failures)` |
| `turn_canceled` | cleared | unchanged | `canceled` |
| `error{message?}` | unchanged | unchanged | `lastErrorMessage`; push_event(agentError); `Notify(failures)`; does not set `errored` |
| `compaction`, `queue_update`, `user_message` | unchanged | unchanged | ignored (drill may show `system`) |
| unknown | unchanged | unchanged | counted |

`toolDetailText`: shell→`command`, read/write/edit→`path`, search→`query`, fetch→`url`, task→`description`. Text tail buffer per agent ≤ 1 KiB, reset on `final`/`turn_started`. With parallel tool calls `currentTool` is the most recently started/updated running call.

### Drill-down buffer (D6, D17, D27)
```
agent.select(key):  drilldown = {target:key, items:[], state: loading}; effects += FetchTimeline(newest page, no `after`)
Timeline(a, items, seqEnd, reset):  if target matches (pages arrive from the client's paging, D39):
        reset → items = map_each(items) -> DrillItem (unknown shapes skipped + counted); else append mapped items
        seqHigh = max(seqHigh, seqEnd); replay events held while loading (drop those with seq <= seqHigh); state = live
        # no paging effect here: the client follows `hasNewer` itself
drill_apply(event, seq?):
    loading|refetching: hold (≤ 2000); return
    if seq <= seqHigh: ignore else seqHigh = max
    assistant/reasoning deltas append to the open block of that kind (new block after `final`/kind change);
    tool_call upserts by callId (status; outputTail ← last 12 lines; terminal output authoritative);
    turn_* append; on turn_canceled/turn_failed running tools close ("canceled"/"error"); error → error item
    if items > 2000: drop oldest, truncatedAtStart = true
TimelineReset(a): items = [], state = refetching, FetchTimeline(newest)           # D27
agent.deselect: drilldown = None (buffer freed); agent.select(other) replaces immediately
```

### Derivations
```
needs_attention(agent) = attention.pendingDialogs > 0 or attention.errored
card_order(filters): cards = agents where !archived, daemon not duplicate, matching daemon (serverId) and status filters
    rank = 0 if needs_attention else {running:1, error:2, idle:3, initializing:3, Unknown:3, closed:4}
    sort by (rank, daemon config order, lastActivity desc nulls last, title asc, agentId asc)       # total order
grouped_order(filters): same scope as card_order; bucket by (serverId, workspace_of(cwd)); groups sorted by (cwd absent last, best card rank, daemon order, label, path); cards inside a group by the card_order sort      # total order; see features/agent-workspace-grouping.md
```
`lastActivity` for sorting is bumped only by status/title changes and by item raise — **not** by `agent_stream` — so cards don't jump while streaming. Status filter values are `AgentStatus` per app-structure; "needs attention" is represented by rank/flag, not a filter chip.
```
usage_totals(filter): scope = filtered cards; of = |scope|; covered = |scope with usage != none|
    tokens = Σ (usage.tokens.total ?? input+output); cost = Σ usage.cost (no conversion); label "N of M agents"        # D16
```

### Bounds and enforcement
| Bound | Value | Enforced by |
|-------|-------|-------------|
| Drill items / pending-while-loading | 2,000 / 2,000 | `drill_apply` |
| Event ring per daemon | 500 | `push_event` |
| Dialog `title` / `message` / each option / options count | 200 / 500 / 120 / 50 | `from_wire` |
| Summary strings | title ≤ 200, label ≤ 120, output line ≤ 200, error ≤ 200 | mapping |
| Assistant text tail | 1 KiB per agent | stream reducer |
| Agents | none in reducers; target 10 × 500 = 5,000 | memory budget |
| Frame size / malformed JSON | L0/L4 | not here |

### Memory and scale (D28, D33)
5,000 `AgentEntry` ≈ ≤ 2 KiB each ≈ 10 MB; rings ≈ 1 MB; drill ≤ ~2 MB; dialogs few. Token deltas for 5,000 agents must be O(1) per event (hash lookup + bounded string ops). The view carries only the visible card window, rail, and totals — not the buffer.

## Data & persistence touchpoints
- Nothing persisted (D8). `settings` mirror comes from `config`; `DaemonSlice` list from config entries (`daemon.add/edit/remove`); removal drops the slice's agents, items and ring.
- `MonitorState` is serde-serialisable (snapshot tests, `document.inspect`); never holds passwords, raw `action` payloads, or prefill text.
- `UiState` (shell) holds filters, rail, dialog, diagnosticsTarget, toasts, focusItem; selection lives in `MonitorState.drilldown.target` (app-structure).

## Error handling & edge cases
| Condition | Expected behavior |
|-----------|-------------------|
| Event for unknown agent | Dialog items still stored (card flag attaches when the agent appears); stream/stats ignored |
| Event from an older connection | `connEpoch` lower than the slice's → dropped before any reducer runs (D38) |
| Duplicate dialog (broadcast + list overlap) | Upsert by `(serverId, requestId)`; one notification |
| `agent_ui_resolved` for unknown request | No-op, silent |
| `expectsResponse:false` or `removed:true` | Not an item |
| Dialog with unknown `method` and `expectsResponse:true` | `Other`, hand-off |
| Select with empty/invalid `options` | hand-off |
| Unknown status/event kind | `Unknown`/ignored, counted |
| Disconnect mid-turn | Stale; rebuild clears `currentTool` unless still running |
| Drill agent leaves snapshot | `CloseDrill` + notice |
| Daemon removed while selected | Drill closed |
| Batch with thousands of events | Applied in order, one revision bump |

## Dependencies on other specs
- [protocol-client.md](protocol-client.md) — `DaemonEvent`, `Snapshot`, outbound allowlist, `RespondUi`.
- [app-structure.md](app-structure.md) — layers, `UiState`/`MonitorState` split, `EngineServices`.
- [agent-monitor.md](../features/agent-monitor.md), [attention-and-interventions.md](../features/attention-and-interventions.md), [notifications-and-tray.md](../features/notifications-and-tray.md), [daemon-health-and-logs.md](../features/daemon-health-and-logs.md).
- pi-studio `swe/features/extension-ui-rpc.md` (dialog semantics), `external-client-fixtures.md` (fixtures drive reducer tests, D22).

## Acceptance criteria
- [ ] Given 10 daemons × 500 agents, then the serialised view holds only the card window + rail + totals and RSS stays < 300 MB. (D28, D33)
- [ ] Given 5,000 stream items for the drilled agent, then `items.len() == 2000`, `truncatedAtStart` is true and the newest item is last. (D17)
- [ ] Given 1,000 events on one daemon, then its ring holds the latest 500 in order. (D17)
- [ ] Given a daemon disconnect, then its agents are `stale` and stay in `card_order`; a later `Snapshot` clears `staleSince` and replaces agents/uiRequests in one reducer step. (D25)
- [ ] Given a `confirm` dialog pending before the dashboard connected and returned in `uiPending`, then after `Snapshot` exactly one `UiRequestItem` exists even if the live broadcast also arrives. (D36)
- [ ] Given a `Snapshot` omitting a previously pending dialog, then it is gone after rebuild. (D8, D25)
- [ ] Given `agent_ui_request` with `expectsResponse:false` (setStatus/notify/setWidget) or `removed:true`, then no item and no notification result. (D36)
- [ ] Given dialogs `confirm`, `select` with options, `select` without options, `input`, `editor`, then answerability is Answerable, Answerable, HandOff, HandOff, HandOff. (D36)
- [ ] Given `timeoutMs`, then `expiresAt` is set, controls disable once expired locally, and only `agent_ui_resolved{timeout}` removes the item. (D36)
- [ ] Given `interrupt` succeeds, then no `uiRequests` entry is removed by the reducer. (D36)
- [ ] Given `turn_started`, `tool_call{running, shell}`, `tool_call{partial, output:"a\nb"}`, then `currentTool.label` is the command and `lastOutputLine == "b"`; `tool_call{completed}` clears `currentTool`; `reasoning` leaves `lastOutputLine` unchanged. (D6)
- [ ] Given `turn_failed`, then `turnState == failed`, `errored == true` and one failures `Notify` effect is returned. (D9)
- [ ] Given any delivery order of stream deltas, `card_order` is identical. (D15)
- [ ] Given mixed statuses and one agent with a pending dialog, then order is attention → running → error → idle → closed with the documented tie-breaks. (D15)
- [ ] Given 3 of 5 agents with usage, then `usage_totals` reports `covered=3, of=5`. (D16)
- [ ] Given two entries with one `serverId`, then agents are counted once; given a changed `serverId`, old agents are removed. (D19)
- [ ] Given a `feed.ingest` batch, then no history entry is created and one revision bump occurs. (D28)
- [ ] Given two running agents and one idle on a connected daemon, when `stats_tick` runs, then exactly two `FetchStats` effects are returned; given a disconnected daemon, none. (D16, D39)
- [ ] Given a slice at `connEpoch` 3, when a `UiRequest` or `AgentStream` tagged epoch 2 arrives, then state is unchanged and no effect is returned; an event tagged 3 is applied. (D38)
- [ ] Property: for arbitrary event sequences all bounds are respected, reducers never panic, `apply` is deterministic and `card_order` is a total order. (D15, D17, D28)

### Test ideas
proptest over arbitrary `DaemonEvent` streams; golden replay of vendored fixtures (`crates/protocol/tests/fixtures/pi-studio/`, D22); perf replay of 10 × 50 streaming agents asserting reducer throughput and ≤ 10 view emits/s through the coalescer (D28, D33).

## TODO(verify)
- Whether Pi cancelling a dialog surfaces as `agent_ui_resolved{aborted}`; the `cancelled` reason is schema-only today (fixture exception).
