# Task 001 — Load harness and budgets

- **Sprint:** sprints/sprint-009-stage1-hardening
- **Status:** backlog
- **Type:** test
- **Area:** cross-cutting (client + ingest + coalescer)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** sprint-008-notifications-tray/task-004

## Goal
Prove stage 1 stays within its throughput and memory budgets under 10 fake daemons × 50 streaming agents, and record idle CPU/RSS against D33.

## Context / why
The dashboard monitors many daemons while idle in the tray; unbounded memory or view-emit storms are the plausible regressions (D28, D33). This turns the bounds in the specs into measured, repeatable checks.

## Scope references
- `swe/specs/architecture/live-state.md` § Memory and scale, § Bounds and enforcement
- `swe/specs/architecture/protocol-client.md` § Backpressure, § Events emitted by L4
- `swe/specs/architecture/app-structure.md` § Threads (view coalescing ≤ 1 emit/100 ms)
- `swe/specs/overview.md` § Purpose (D33)
- `crates/testkit/` (scripted fake daemon from sprint-004/task-002; add load scenario)
- `crates/client/tests/` or a new `apps/pistudiodashboard/src-tauri/tests/load.rs`
- `docs/development.md` (how to run the load test and idle measurement)

## What to build
- Scenario test: 10 fake daemons (tokio fake WebSocket servers from testkit), each 50 agents streaming `agent_stream` events (deltas, tool calls, turn boundaries) at a stated rate, wired through the **real** client → feed pump → `feed.ingest` on an owner thread → view coalescer. Run for a bounded wall-clock duration (≥ 30 s; env var to lengthen).
- Assertions: view emits ≤ 10 per second (sampled per 1 s window, with the trailing emit); total retained model size respects the caps (event ring, drill buffer, per-agent activity, uiRequests) — assert via `MonitorState::validate()` and a measured RSS delta bound; reducer throughput meets a documented floor events/s (record the observed number and set the floor with generous headroom); no channel growth (bounded channel depth never exceeds its bound); a slow consumer causes drops/coalescing, not memory growth.
- Idle measurement script/procedure (`xtask` subcommand or documented shell steps): launch the app with a local fake/real daemon connected and idle, sample CPU% and RSS for ≥ 60 s; compare with D33 (< 2% CPU, < 300 MB RSS) and record the numbers in the task summary and `docs/development.md`. A miss is a finding to report, not something to hide by raising the threshold.
- Keep the load test out of the default `cargo xtask ci` path if it exceeds a reasonable runtime (mark `#[ignore]` and provide a reduced smoke variant that does run in CI); the full run is executed once for this task and recorded.

## Out of scope
- Real-daemon pass — sprint-009/task-002.
- Docs completion — sprint-009/task-003.
- Stage 2 health/log streaming load (200 lines/s) — covered by sprint-010 tasks.

## Acceptance criteria
- [ ] Given 10 fake daemons × 50 streaming agents, then the view emits at most 10 times per second in every 1 s window.
- [ ] Given the same run, then every bounded structure stays within its documented cap and `validate()` passes at the end.
- [ ] Given a deliberately slow view consumer, then memory stays bounded (events are coalesced or dropped, never queued without bound).
- [ ] Given the idle measurement on the target machine, then CPU and RSS numbers are recorded and compared with D33 (< 2% / < 300 MB) in the task summary.
- [ ] Given the reduced smoke variant, then it runs inside `cargo xtask ci` within a few seconds.

## Test / verification plan
- Run: `cargo test -p <crate> --test load -- --ignored --nocapture` for the full run; record emits/s, throughput, RSS.
- Idle measurement per the documented procedure; paste results into the summary.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- Absolute thresholds come from D33 and the spec bounds; reducer-throughput floor is chosen from the first measured run with headroom (record both).
- TODO(verify) from live-state § Memory and scale: confirm the stated per-agent memory estimates against the measurement.
