# Task 001 — Ingest crate: connection reducers and view derivations — Summary

- **Sprint:** sprints/sprint-002-domain-cutover
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
New L2 crate `pistudiodashboard-ingest` (depends only on `model`): `apply` / `apply_batch` / `apply_in_place` (epoch gate D38, `ConnState`, `Hello` + identity D19, `Health`, `Snapshot` rebuild, `mark_stale`, `Rtt`, `Diagnostics`) and derivations `card_order`, `usage_totals`, `daemon_status_counts` with `Filters`. `model` gained `event.rs` (`DaemonEvent`, `DaemonEventKind`, `AgentListItem`, `PendingUi`, `ServerCaps`, `CountersSnapshot`) and `effect.rs` (`Effect`, `NotifyKind`, `ToastKind`). `ConnState`/`DegradedReason` were already model-owned (task-003).

## Files created / changed
| File | Change |
|------|--------|
| `crates/ingest/Cargo.toml`, `src/{lib,apply,identity,rebuild,derive,keys}.rs` | created |
| `crates/ingest/tests/{reducers.rs,properties.rs,common/mod.rs}` | created (10 reducer tests, 1 proptest) |
| `crates/model/src/{event,effect}.rs` | created |
| `crates/model/src/{lib,daemon,agent,state}.rs` | extended (see deviations) |
| `Cargo.toml` | one line: `proptest` in `[workspace.dependencies]` |

## How it satisfies the scope
Per live-state.md § Top-level reducer / Identity / Rebuild / Stale / Derivations. Decisions and model-type deviations:
- `DaemonSlice` gained `conn_since`, `next_retry_in_ms`, `caps: ServerCaps`, `tls_in_use`, `is_loopback`; `DaemonCounters` expanded to the diagnostics set; `MonitorState.dropped_events: u64` (unknown entry, or snapshot before `Hello` / for a duplicate slice); `ConnState::as_str`; `AgentStatus` derives `Ord`.
- `Hello` identity comes from `DaemonEvent.server_id` (`ServerInfoView` has no id field).
- `AgentListItem.labels` is `Vec<String>` (client flattens the wire map). `PendingUi` carries `title/message/options` already extracted by the client (no raw payload crosses into the model, D29/D32). `model_provider`/`thinking_level` are carried but not yet stored on `AgentEntry`. TODO(verify) when sprint-006 maps them.
- `ui_pending` conversion is minimal as specified: bounded text, `kind = Other`, `HandOff`, `expires_at` derived; classification is sprint-007/task-001.
- Events from an older epoch are dropped silently (state unchanged, not counted). A duplicate slice owns no agents: its snapshots are dropped and counted and its disconnect does not stale the owner's agents. `Snapshot` with a missing drill target also clears `state.drilldown` in addition to emitting `CloseDrill` (idempotent for the engine); likewise on "daemon replaced".
- An agent with wire status `error` is built with `attention.errored = true`.
- Known limit (documented in `identity.rs`): when an owner changes id, an existing duplicate re-resolves at its next `Hello`.
- `Filters.daemons` are config `entry_id`s; `card_order` iterates daemons in config order and uses key-range access (O(log n + k) per daemon).
- `xtask` registration (`TABLE`, `WASM_CRATES`) is owned by task-005 per the parallel-work rules: not done here.

## Build & test results
```
$ cargo test -p pistudiodashboard-ingest -p pistudiodashboard-model   # ingest 10 + 1 proptest, model 9: all passed, 0 failed
$ cargo clippy -p pistudiodashboard-ingest -p pistudiodashboard-model --all-targets -- -D warnings   # clean
$ cargo fmt -p pistudiodashboard-ingest -p pistudiodashboard-model --check   # clean
$ cargo check -p pistudiodashboard-ingest --target wasm32-unknown-unknown   # ok (model, protocol too)
```
New dev-dependency `proptest 1.12.0` (default-features off, `std` only): MIT OR Apache-2.0; transitive tree (rand, rand_core, rand_xorshift, unarray, chacha20, cpufeatures, core_detect, getrandom, regex-syntax, bitflags, num-traits) checked MIT/Apache-2.0 from the crate manifests. Dev-only; not in the shipped graph.

## Acceptance criteria
- [x] epoch 2 dropped (state equal, no effects), epoch 3 applied, `ConnState` epoch 4 raises (stale_epoch_is_dropped_and_newer_conn_state_raises_it)
- [x] backoff -> stale, still in `card_order`; snapshot clears `stale_since`, replaces agents/dialogs in one step (disconnect_marks_stale_and_snapshot_replaces_in_one_step)
- [x] omitted dialog gone; duplicate pending delivered twice -> one item; `expectsResponse:false`/`removed` excluded (dialogs_upsert_by_key_and_filter_non_items, ...snapshot...)
- [x] same `server_id` counted once; changed id removes old agents (duplicate_server_id_counts_once_and_changed_id_removes_old)
- [x] drill target present -> `refetching` + one `FetchTimeline`; missing -> `CloseDrill("agent gone")` (snapshot_refreshes_drill_target)
- [x] `FetchStats` for exactly running / no-usage agents (snapshot_fetches_stats_only_for_running_or_without_usage)
- [x] `card_order` ranks and tie-breaks, archived and duplicate excluded (card_order_ranks_with_tie_breaks_and_excludes_archived)
- [x] `usage_totals` covered 3 of 5, sums (usage_totals_cover_three_of_five)
- [x] unknown `entry_id` ignored and counted (unknown_entry_is_dropped_counted_and_creates_nothing)
- [x] property: total, deterministic, `validate()` ok, no duplicate cards, batch == fold (apply_is_total_deterministic_and_bounded)
- [ ] `cargo xtask wasm` / `layers`: not run (xtask is task-005's; ingest not registered yet). Equivalent wasm32 check passes; ingest depends only on `model`.

## Smoke against pi-studio
N/A: pure reducer crate with no daemon-facing surface; nothing was run against 127.0.0.1:6790.

## Follow-ups / TODO(verify)
- Register `ingest` as `Layer(2)` in `TABLE` and in `WASM_CRATES` (task-005), then run `cargo xtask layers`/`wasm`/`ci`.
- Map `model_provider`/`thinking_level` onto `AgentEntry` when needed (sprint-006).
