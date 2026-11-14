# Task 005 — Drill-down engine

- **Sprint:** sprints/sprint-006-agent-monitor
- **Status:** done
- **Type:** feature
- **Area:** crates/engine (L5) + crates/ingest (L2) + crates/client (L4)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-002

## Goal
Implement agent selection, the drill-down buffer reducers and the timeline fetch/paging/refetch flow.

## Context / why
A live view of one agent's current turn (D6): seeded from `fetch_agent_timeline_request`, then folded with live deltas, bounded at 2,000 items (D17), closing with a notice when the agent disappears (D27).

## Scope references
- swe/specs/architecture/live-state.md § Drill-down buffer, § Per-event reducers (Timeline/TimelineReset, AgentArchived/Deleted), § Rebuild-on-connect (drilldown), § Bounds
- swe/specs/architecture/protocol-client.md § Drill-down timeline fetch (D6, D17, D27)
- swe/specs/features/agent-monitor.md § Drill-down panel (behavior)
- swe/specs/architecture/app-structure.md § Command registry (`agent.select`/`agent.deselect`)
- `crates/engine/src/agent_cmds.rs` (new)
- `crates/ingest/src/drill.rs` (new)
- `crates/model/src/` (`DrillBuffer`, `DrillItem`)
- `crates/client/src/timeline.rs` (new)

## What to build
- **Commands** (`crates/engine/src/agent_cmds.rs`, registry): `agent.select {serverId, agentId}` — enabled when the agent exists and is not archived (else disabled 'agent not available'); sets `MonitorState.drilldown = {target, items:[], state:loading}` and returns effect `FetchTimeline{serverId, agentId}` (newest page); selecting another agent replaces the buffer immediately (previous freed). `agent.deselect {}` — enabled when a drill-down is open; frees the buffer. Selection lives in `MonitorState.drilldown.target` (not `UiState`), per the spec decision.
- **Effects (D39: engine decides, client pages)**: the executor (sprint-002) turns `FetchTimeline{serverId, agentId}` into one `send_to_daemon(FetchTimeline{agentId})` request. The client (`crates/client/src/timeline.rs`) performs the paging for that one request: first `fetch_agent_timeline_request{cursor:null, direction:"before", limit:200}` (newest page, verified in pi-studio `timeline-store.ts`), then while `hasNewer` → `direction:"after"`, `cursor = endCursor`, until `!hasNewer` or 2,000 items; a newer `FetchTimeline` for the same daemon cancels paging still in progress. It emits `Timeline{agentId, items, reset:true, seqEnd}` for the first page and `reset:false` for later pages (items mapped via the L0 types from task-001). The client never starts a fetch on its own (not on `agent_timeline_reset`, not on reconnect).
- **Reducers** (`crates/ingest/src/drill.rs`, pure): `Timeline(a, items, seqEnd, reset)` when the target matches: `reset` → replace items, else append; map items to `DrillItem` (`assistant{text}`, `reasoning{text}`, `tool{callId, kind, label, status, outputTail? (last 12 lines, each ≤ 500 chars)}`, `error{message}`, `turn{kind}`, `system{text}` for user_message/compaction; unknown shapes skipped + counted); `seqHigh = max(seqHigh, seqEnd)`; replay events held while loading (drop those with `seq <= seqHigh`); `state = live`; **no paging effect** (the client follows `hasNewer`). `drill_apply(event, seq?)` (called from the `AgentStream` reducer when `target == agent`): while `loading`/`refetching` hold events (≤ 2,000); otherwise ignore `seq <= seqHigh`, else `seqHigh = max`; assistant/reasoning deltas append to the open block of that kind (new block after `final`/kind change); `tool_call` upserts by `callId` (status, `outputTail` ← last 12 lines; terminal output authoritative; partials supersede earlier output by callId); `turn_*` append; on `turn_canceled`/`turn_failed` running tools close ('canceled'/'error'); `error` → error item; ephemeral events without `seq` are never used for dedupe; when `items > 2000` drop the oldest and set `truncatedAtStart`.
- **Races/lifecycle**: `TimelineReset(a)` for the target → `items = []`, `state = refetching`, `FetchTimeline(newest)`; `Snapshot` for the target's daemon (reconnect) → `state = refetching` + `FetchTimeline` if the agent is in the new list and not archived, else `CloseDrill("agent gone")`; `AgentArchived`/`AgentDeleted`/daemon entry removal/duplicate-ness → effect `CloseDrill{reason}` executed by the owner (clears `drilldown.target`, toast 'agent no longer on <daemon>' / archived / deleted notice); the duplicate-`serverId` and changed-`serverId` cases close the drill (sprint-002 reducers call it). Daemon disconnect: the buffer is retained read-only and flagged `stale`; no live updates; refetch on reconnect.

## Out of scope
- Drill-down panel rendering (task-006), and the interrupt/dialog buttons in its header (sprint-007).
- Paging beyond what was fetched (non-goal: no history browsing).
- Markdown rendering (default plain text).

## Acceptance criteria
- [ ] Given a selected agent, when the first response (items + `seqEnd`) arrives, then items are mapped, `state == live`, and a stream event with `seq <= seqEnd` that arrived while loading is dropped while a later one is applied. (D27)
- [ ] Given `hasNewer:true` on the first page, then a follow-up `direction:"after"` request with `cursor = endCursor` is sent until `hasNewer:false` and the total fetched stops at 2,000 items. (D17)
- [ ] Given 5,000 stream items for the drilled agent, then `items.len() == 2000`, `truncatedAtStart` is true and the newest item is last. (D17)
- [ ] Given a drilled agent and `agent_timeline_reset`, then the buffer clears, `state == refetching`, a fresh `FetchTimeline` runs and a `Timeline{reset:true}` follows. (D27)
- [ ] Given the selected agent is archived or deleted, then a `CloseDrill` effect is returned, the owner clears the target and shows the notice; given the daemon entry is removed, then the drill closes. (D27)
- [ ] Given a disconnect, then the buffer is kept read-only with `stale`; after reconnect it refetches; given the agent vanished in the new `Snapshot`, then the drill closes with 'agent no longer on <daemon>'.
- [ ] Given `agent.select` on an archived or unknown agent, then it is disabled with 'agent not available'; given `agent.select` of another agent, then the previous buffer is replaced immediately.
- [ ] Given a `tool_call` upsert by `callId`, then status and a ≤ 12-line `outputTail` update in place; given `turn_failed` with a running tool, then the tool closes as 'error'.
- [ ] Given delta events without `seq` (partial tool calls), then they are applied (never used for dedupe).

## Test / verification plan
- `cargo test -p pistudiodashboard-ingest` (drill reducers incl. proptest bounds and race ordering), `cargo test -p pistudiodashboard-engine` (commands, enable reasons, effect execution with a recording `send_to_daemon`) and `cargo test -p pistudiodashboard-client` (timeline paging against the fake daemon).
- Add `agent.select`/`agent.deselect` params to the `panic_hunt` seed and `testkit::adversarial_params`.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- Resolved in pi-studio (spec TODO): `fetch_agent_timeline_request` accepts `direction: before|after`; without a cursor `before` returns the newest page; only `after` has a fixture.
- TODO(verify) (agent-monitor): drill-down text rendering scope (plain vs minimal markdown) — default plain.
- The fixture `fetch_agent_timeline_request.json` uses `direction:"after"` with `limit:100`; the dashboard asks 200 per page.
