# Task 001 — Agent wire types and client events — Summary

- **Sprint:** sprints/sprint-006-agent-monitor
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
Protocol types (`stream.rs`, `agent.rs`, `stats.rs`, `timeline.rs`) with tolerant `from_value`/`from_message` decoding and caps; model event kinds (`AgentUpdate` with double-option title, `AgentStatus`, `AgentArchived`, `AgentDeleted`, `AgentStream`, `TimelineReset`, `Stats`, `Timeline`); client mapping of every agent push, foldable-delta shedding under queue pressure, stats executor (<=4 in flight, dedupe, wait queue), timeline executor (first page `before`/200, `FetchTimelinePage` `after` pages until `!hasNewer` or 2000 items, newer job cancels older).

## Files created / changed
crates/protocol/src/{stream,agent,stats,timeline,lib}.rs, tests/agent.rs; crates/model/src/{event,agent,drill,ring,state,lib}.rs, tests/events_json.rs; crates/client/src/{agents,emitter,requests,conn,sync,lib}.rs, tests/{agents,stats_polling,live_daemon,support/mod}.rs, Cargo.toml (dev-dep ingest).

## How it satisfies the scope
Each acceptance criterion in the task file is covered by the tests listed below (golden fixture replay, unit and property tests, client tests against the fake daemon, live smoke). `agent.select` / `agent.deselect` / `feed.tick` are also covered by the generic registry fuzz (`panic_hunt`).
**Real-daemon finding:** the daemon returns collapsed timeline items: `assistant` / `tool_call` rows carry `events[]` with `sourceSeqStart..End`, only `other` rows carry one `event`. `TimelinePage::from_message` now flattens them (source_seq = `sourceSeqEnd`), test `collapsed_items_are_flattened_to_their_events`. Fixtures had only `other` rows.
**Deviation:** no proactive 50 ms delta folding in the client (it cannot know which agent is drilled); foldable `AgentStream` events are shed only when the consumer queue is full (`droppedEvents` counted); attention events are never shed. Drill folding happens in ingest.

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
