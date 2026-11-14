# Task 003 — Stats polling and usage

- **Sprint:** sprints/sprint-006-agent-monitor
- **Status:** done
- **Type:** feature
- **Area:** crates/client (L4) + crates/ingest (L2)
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Fetch `agent_session_stats_request` on the D16 schedule — decided by the engine, executed by the client (D39) — and fold results into usage with the 'N of M agents' totals.

## Context / why
Per-card tokens/cost and a never-misleading totals bar (D16): partial sums are labelled 'N of M agents' and '—' means no live session.

## Scope references
- swe/specs/architecture/protocol-client.md § Stats polling (D16, D39)
- swe/specs/architecture/live-state.md § Per-event reducers (Stats), § Derivations (usage_totals)
- swe/specs/features/agent-monitor.md § Usage display
- `crates/client/src/stats.rs` (new), `conn.rs`
- `crates/ingest/src/{reduce_stats.rs,derive.rs}`, `crates/engine/src/feed_cmds.rs` (tick), `apps/pistudiodashboard/src-tauri/src/bridge.rs` (owner-loop tick driver)
- `crates/model/src/` (`UsageSnapshot`)

## What to build
**Engine = the only scheduler (D39).** Reducers already return `FetchStats` on `Snapshot` (sprint-002/task-001: every running agent and every agent without usage), on `turn_completed`/`turn_failed` and for a new placeholder agent (task-002). Add the periodic trigger: a pure `stats_tick(state, now) -> effects` in `crates/ingest` returning `FetchStats` for every `running` agent of every connected (`live`) daemon, and an internal engine command (e.g. `feed.tick {now}`, no menu, registered like `feed.ingest`) that the app binary's owner loop calls every 10 s while at least one connected agent is running. The effect executor (sprint-002/task-002) sends `send_to_daemon(SessionStats{agentId})`.
**Client = executor only.** `crates/client/src/stats.rs`: on an outbound `SessionStats(agent)`: drop it if a request for that agent is already queued or in flight (≤ 1 per agent); at most 4 in flight per daemon; nothing is sent while the connection is not `live` (dropped, not queued); a response with `payload.ok == false` → `Stats{agentId, NoSession}`, otherwise `Stats{agentId, Usage{tokens?, cost?, contextUsage?}}`; never retried and never scheduled by the client itself.
**L2 reducer**: `Stats(a, Usage)` → `usage = {tokens?, cost?, contextUsage?, takenAt = event.at}`; `NoSession` → `none`; the rebuild carry-over keeps `takenAt` ('as of'). **Derivation** `usage_totals(filter)` (extends sprint-002/task-001): scope = filtered cards, `of = |scope|`, `covered = |scope with usage != none|`, `tokens = Σ (tokens.total ?? input + output)`, `cost = Σ cost` (no conversion, bare number), `label "N of M agents"`; also when covered == of the label stays 'M of M agents'.

## Out of scope
- Card/status-bar rendering of usage (task-004).
- Closing/closing-agent policy beyond the one connect-time attempt.
- Drill-down stats header (reuses the same usage; task-006).

## Acceptance criteria
- [ ] Given two running agents and one idle agent on a live daemon, when `stats_tick` runs, then exactly two `FetchStats` effects are returned; given a disconnected daemon, none. (D16, D39)
- [ ] Given a `SessionStats` request for an agent that already has one queued or in flight, then no second request is sent; given 500 requests, then at most 4 are in flight per daemon; given a response with `ok:false`, then `Stats{NoSession}` is emitted and no retry occurs. (D16, D33, D39)
- [ ] Given a fake daemon and a recording owner loop with paused time, when an agent runs for 25 s and then completes its turn, then ≥ 2 periodic stats requests plus one after `turn_completed` reach the daemon; an idle agent gets exactly one request (after `Snapshot`) and no periodic ones. (D16)
- [ ] Given `tokens:{total:1200}` without `cost`, then `usage.tokens == 1200`, `cost == None`; given `tokens:{input:100, output:50}` without `total`, then tokens = 150.
- [ ] Given 3 of 5 agents with usage, then `usage_totals` reports `covered=3, of=5` and the label 'N of M agents' (3 of 5 agents). (D16)
- [ ] Given an agent with no live session, then usage is `none` and shown as '—' by consumers; a usage older than its refresh window keeps `takenAt` for 'as of'.

## Test / verification plan
- `cargo test -p pistudiodashboard-client` (executor dedupe/concurrency with a fake daemon and paused tokio time), `cargo test -p pistudiodashboard-ingest` (`stats_tick`, reducer, totals table tests), `cargo test -p pistudiodashboard-engine` (tick command).
- Add a case to the proptest from task-002 covering `Stats` events.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (agent-monitor): whether usage should be polled for `closed` agents (one attempt at connect is proposed).
- TODO(verify) (agent-monitor): the stats payload has no currency field; render bare number.
- TODO(verify) (live-state): whether to bump `takenAt` for 'as of' only on success.
