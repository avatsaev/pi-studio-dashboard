# Task 005 — Drill-down engine — Summary

- **Sprint:** sprints/sprint-006-agent-monitor
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
Commands `agent.select {serverId,agentId}` (Disabled "agent not available" for unknown/archived; executes `FetchTimeline`) and `agent.deselect` (enabled only with an open drill); ingest `drill_select`/`drill_deselect`, held-event replay, text-block folding, tool upsert with 12-line tail, 2000-item cap, `TimelineReset`, disconnect -> refetch; engine test feeding JSON events end to end.

## Files created / changed
crates/engine/src/{agent_cmds,lib,commands}.rs; crates/ingest/src/{drill,reduce_agent,rebuild}.rs, tests/drill.rs; crates/model/src/drill.rs.

## How it satisfies the scope
Each acceptance criterion in the task file is covered by the tests listed below (golden fixture replay, unit and property tests, client tests against the fake daemon, live smoke). `agent.select` / `agent.deselect` / `feed.tick` are also covered by the generic registry fuzz (`panic_hunt`).
**Deviations:** drill state stays `Live` after the first page (events arriving during later `after` pages could misorder; documented); no engine-driven loading error/retry when the first page fails (request error toast only). Flattening of collapsed items (task-001) is what makes the live drill buffer show the assistant message.

## Build & test results
```
$ cargo test -p pistudiodashboard-protocol -p pistudiodashboard-model -p pistudiodashboard-ingest   -> all ok (protocol agent 11, model events_json 2, ingest agents 10 / drill 8 / stats 5 / properties / reducers 10)
$ cargo test -p pistudiodashboard-client   -> agents 8, connection 14, auth_counters 9, requests 7, sync 5, secrets 1, stats_polling 1, lib 24: all ok
$ cargo test -p pistudiodashboard-engine --lib   -> 36 passed
$ cargo clippy -p <protocol|model|ingest|client|engine> --all-targets -- -D warnings   -> clean
$ cargo xtask ci   -> fmt, web, clippy, layers, test (297 passed, 6 skipped) ok; panic_hunt: see note
```
Note: `panic_hunt --ignored` is SIGKILLed (OOM) in this container before reaching any command's
result: the testkit `adversarial_params()` builds a ~10 GB `hostile_events` value for every
command. A reduced run of the new commands (`agent.*`, `feed.tick`, the first 6 adversarial
params; the hostile-events one only matters for `feed.ingest`) passed:
`cargo test -p pistudiodashboard-engine --test <temp copy> -- --ignored` -> 1 passed.

## Smoke against pi-studio
Command (dev daemon, mock provider, `ws://127.0.0.1:6790/ws`):
```
PI_STUDIO_SMOKE=127.0.0.1:6790 PI_STUDIO_SMOKE_PROMPT="hello" PI_STUDIO_SMOKE_CLI="node /home/avatsaev/DEV/avatsaev/pi-studio/packages/cli/dist/cli.js -H 127.0.0.1:6790 --home /tmp/pi-studio-cli" \
  cargo test -p pistudiodashboard-client --test live_daemon live_daemon_agent_lifecycle -- --ignored --nocapture
```
```
see output below
```
Run with prompts `hello`, `#stream reasoning`, `#stream fail` against the dev mock daemon (all `1 passed`).
Observed: snapshot of 12 agents, Stats for all; new agent Initializing -> Idle -> Running; stream seq 0..3
`UserMessage`, `TurnStarted`, `AssistantMessage{"echo: hello", final:true}`, `TurnCompleted`, then idle.
Reasoning run: `Reasoning{"mock reasoning"}`, `Reasoning{final}`. Fail run: `TurnFailed{"mock turn failure"}`
-> card Error, `errored=true`, `last_error_message` set. `agent.select` -> `Timeline items=4 reset=true seqEnd=3`,
drill `state=Live seqHigh=Some(3)`, `state.validate()` Ok.

Captured final state (hello run): `status=Idle turn=Completed lastLine=Some("echo: hello")`, drilldown items `System "hello"`, `Turn Started`, `Assistant "echo: hello"`, `Turn Completed`.
