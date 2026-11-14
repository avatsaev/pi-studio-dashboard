# Task 003 — Model crate: `MonitorState` and its bounds — Summary

- **Sprint:** sprints/sprint-001-core-crates
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
New L1 crate `pistudiodashboard-model` (depends only on `protocol`, plus serde/serde_json/thiserror): `MonitorState` and all component types (daemon slice, `ConnState`, `DegradedReason`, agent/activity/usage/attention, `UiRequestItem`, `DrillBuffer`/`DrillItem`, `EventRecord`, `SettingsMirror`, empty `Observability`), the bounded `Ring<T, CAP>`, every cap as `pub const`, `cap_string` (wraps L0 `cap_str`) and `MonitorState::validate() -> Result<(), Invalid>`.

## Files created / changed
| File | Change |
|------|--------|
| `crates/model/Cargo.toml` | created |
| `crates/model/src/{lib,caps,ring,settings,daemon,agent,dialog,drill,state}.rs` | created, with unit tests (ring, state, caps) |

## How it satisfies the scope
Per live-state.md § MonitorState / Bounds. Decisions and deviations:
- `Ring<T, const CAP: usize>` (capacity is a const generic, `Ring::<_, 500>::new()`), not a runtime `Ring::new(500)`: serde deserialisation of a plain sequence must know the bound to truncate hostile input. Aliases are not needed; fields use `Ring<EventRecord, EVENT_RING>` / `Ring<DrillItem, DRILL_ITEMS>`.
- Map keys: `agents`/`ui_requests` are `BTreeMap<String, _>` keyed `"<serverId>\u{1f}<id>"` via `agent_key`/`ui_request_key` (O(log n), JSON-safe); `daemons` is a `Vec` in config order with unique `entry_id` (checked by `validate`).
- Caps are byte limits (UTF-8-boundary truncation), documented in `caps.rs`.
- `drilldown.target` may name an agent absent from `agents` (stale/removed); reducers close it, `validate` does not require existence.
- `DrillItem` is tagged by `type` (the `tool` variant has its own `kind` field). `UsageSnapshot` is tagged by `state`.
- Extras beyond the field table: `UiRequestItem.stale` (stale marking spec), `ConnErrorView`, `TokenUsage`, `ContextUsage`.
- `xtask/src/layers.rs` `TABLE` registration is task-005's (parallel-work rule: only `crates/model/**` touched); the workspace dependency line already existed.

## Build & test results
```
$ cargo test -p pistudiodashboard-model                                     # 9 passed, 0 failed
$ cargo clippy -p pistudiodashboard-model --all-targets -- -D warnings      # clean
$ cargo fmt -p pistudiodashboard-model --check                              # clean
$ cargo check -p pistudiodashboard-model --target wasm32-unknown-unknown    # ok
```

## Acceptance criteria
- [x] Default state round-trips, `validate()` Ok (default_round_trips_and_validates)
- [x] Populated state round-trips with camelCase keys and JSON-safe map keys (populated_round_trips_with_camel_case_and_json_safe_keys)
- [x] 1,000 pushes into a 500 ring keep latest 500 in order; push reports the drop (keeps_latest_in_order_and_reports_drops)
- [x] 5,000 pushes into drill ring -> 2000, newest last (drill_ring_holds_newest_2000)
- [x] Over-long JSON ring truncated to newest cap (deserialisation_truncates_to_newest, hostile_ring_input_is_truncated)
- [x] Unknown status -> `Unknown`; sparse agent deserialises (unknown_status_and_sparse_agent_are_lenient)
- [x] Oversized title, duplicate entry_id, over-capacity ring, too many options, key mismatch -> matching `Invalid` (validate_flags_violations)
- [ ] `cargo xtask wasm` / `cargo xtask layers`: not run (xtask is out of this task's scope and `model` is not in `TABLE` until task-005); the equivalent `cargo check --target wasm32-unknown-unknown` passes and the only workspace dependency is `protocol`.

## Smoke against pi-studio
N/A: pure data crate with no daemon-facing surface; nothing was run against 127.0.0.1:6790.

## Follow-ups / TODO(verify)
- `ContextUsage` field set to verify against the stats payload (sprint-006).
- Assistant/reasoning text in `DrillItem` has no per-item cap in the spec; ingest bounds growth via the 2,000-item ring.
- Parent runs `cargo xtask ci` / `layers` after task-005 registers `model`.
