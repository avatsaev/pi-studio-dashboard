# Task 001 — Ingest crate: connection reducers and view derivations

- **Sprint:** sprints/sprint-002-domain-cutover
- **Status:** done
- **Type:** feature
- **Area:** L2 `crates/ingest` (pure reducers), `crates/model` (event/effect types)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** sprint-001-core-crates/task-003

## Goal
Create `pistudiodashboard-ingest` (L2) with `apply(state, event, now) -> (state, effects)` implementing the top-level reducer and the connection-level events (`ConnState`, `Hello` + identity, `Health`, `Snapshot` rebuild, stale marking, `Rtt`/`Diagnostics`), plus the derivations `card_order`, `usage_totals`, `daemon_status_counts`.

## Context / why
Live data enters the engine only as `DaemonEvent` batches folded by pure reducers (reducers pure, effects executed by the owner thread; app-structure § Threads). Building the connection half first lets the engine cutover (task-002) wire `feed.ingest` against real reducers and lets sprint-004's client emit exactly these events. Agent, dialog, stats and drill-down reducers are added by sprints 006/007; this task leaves their `DaemonEvent` kinds out of the enum (they are added together with their reducers) so nothing is a stub.

## Scope references
- `swe/specs/architecture/live-state.md` § `DaemonEvent`, § Top-level reducer, § Identity (D19), § Rebuild-on-connect, § Stale marking, § Derivations, § Engine surface (`Effect`), § Error handling & edge cases, § Acceptance criteria (connEpoch, stale, Snapshot, duplicates, usage totals, card order)
- `swe/specs/architecture/protocol-client.md` § Events emitted by L4 (event shapes), § Diagnostics counters
- `swe/specs/architecture/app-structure.md` § Layer / crate map (L2 row; allowed deps: protocol, model), § Threads (Effects table)
- `swe/grills/daemon-monitoring-dashboard.md` D8, D15, D16, D19, D25, D38
- `crates/model/**` (from sprint-001/task-003), `xtask/src/layers.rs` (`TABLE`, and `WASM_CRATES` from sprint-001/task-005)
- Create `crates/ingest/{Cargo.toml,src/lib.rs,src/apply.rs,src/identity.rs,src/rebuild.rs,src/derive.rs}`; add `crates/model/src/event.rs` (and `effect.rs`)

## What to build
- **Types in `model`** (so L4 and L2 both use them; client depends on model, not on ingest internals): `DaemonEvent { entry_id, server_id: Option<String>, conn_epoch: u64, seq: u64, at: i64, kind: DaemonEventKind }` and `DaemonEventKind` with exactly the variants this task reduces: `ConnState{state, since, last_error, next_retry_in_ms}`, `Hello{info: ServerInfoView, caps: ServerCaps, degraded, tls_in_use, is_loopback}`, `Health{reachable, at}`, `Snapshot{agents: Vec<AgentListItem>, ui_pending: Vec<PendingUi>}`, `Rtt{ms}`, `Diagnostics(CountersSnapshot)`. Model-owned plain structs `AgentListItem` (agent id, status string, title, cwd, labels, last_activity, provider, model, model_provider, thinking_level — as in `list_agents_response`) and `PendingUi` (the `agent_ui_request`-shaped fields) so L2 does not depend on typed protocol messages that arrive in sprint-004/005; the client maps wire → these. Mark the enum `#[non_exhaustive]`-free but extendable: later sprints add variants and their reducers in one change.
- `Effect` enum in `model` exactly per live-state/app-structure: `Notify{kind, server_id, item_id}`, `ResolveNotification{server_id, item_id}`, `Toast{kind, text}`, `CloseDrill{reason}`, `FetchStats{server_id, agent_id}`, `FetchTimeline{server_id, agent_id}` (one logical (re)load; the client does the paging, D39). This task emits `FetchStats`, `FetchTimeline` and `CloseDrill` (from `Snapshot` rebuild) and `Toast` (identity change "daemon replaced"); `Notify`/`ResolveNotification` are emitted by sprint-006/007 reducers.
- **`ingest::apply(state: &MonitorState, event: &DaemonEvent, now: i64) -> (MonitorState, Vec<Effect>)`** (or `&mut` + effects; keep it pure and total — never panics, unknown entry ⇒ drop and count). Also `apply_batch(state, &[DaemonEvent], now)` used by the engine: applies in order, one published state per batch.
  - Entry/slice missing (entry removed) → event ignored, counted in an ingest-level `dropped` counter on the state.
  - **connEpoch rule (D38):** `event.conn_epoch < slice.conn_epoch` → dropped before any reducer; a `ConnState` with a higher epoch raises `slice.conn_epoch`.
  - `ConnState`: set `conn`, `last_error`; entering backoff/auth_failed/host_rejected with a known `server_id` → `mark_stale`; push an `EventRecord(connState)` with fixed-vocabulary summary.
  - `Hello`: `resolve_identity` (D19) — duplicate `server_id` marks the later entry `duplicate_of` and drops that slice's agents/dialogs (no double counting); changed `server_id` at the same entry removes the old agents/dialogs/events, closes a drill target on the old server (`CloseDrill("daemon replaced")`) ; first `Hello` processed owns the `server_id`; store `info`, `degraded`.
  - `Health`: set `slice.health`.
  - `Snapshot` → `rebuild`: atomic replace of that daemon's agents and `ui_requests` in a single assignment (no frame with an empty slice); carry-over of usage (keeps `taken_at`) and of `activity` only when still running; recompute attention caches (`pending_dialogs` from the new dialog map; `UiRequestItem` fields are populated by a minimal conversion limited to what the model defines — full classification `from_wire` is sprint-007/task-001; this task keeps `ui_pending` handling to “replace the slice's dialog map with converted items that have `expectsResponse` and are not `removed`”, using the model-level struct, and leaves method classification as `Other`/hand-off until task 007/001 refines it — document this); clear `stale_since` and agents' `stale`; effects `FetchStats` for agents running or without usage; if `drilldown` targets this server: `refetching` + `FetchTimeline` when the agent is still present and not archived, else `CloseDrill("agent gone")`.
  - `Rtt`/`Diagnostics`: update the slice counters only.
  - `mark_stale`: `stale_since ??= now`, all slice agents `stale = true`, dialog items kept but flagged, in-flight flags cleared, drill buffer flagged stale; agents never dropped.
- **Derivations** (pure fns in `derive.rs`, never stored): `card_order(state, &Filters) -> Vec<(server_id, agent_id)>` (agents `!archived`, daemon not a duplicate, daemon/status filters; rank 0 needs-attention, running 1, error 2, idle/initializing/Unknown 3, closed 4; sort by `(rank, daemon config order, last_activity desc nulls last, title asc, agent_id asc)` — total order); `usage_totals(state, &Filters) -> {tokens, cost, covered, of}`; `daemon_status_counts(state, entry_id) -> BTreeMap<AgentStatus, usize>`. `Filters { daemons: Vec<String>, statuses: Vec<AgentStatus> }` (empty = all).
- Crate setup: depends on `protocol` (only if a helper is needed — otherwise just `model`), `serde`/`serde_json` as needed; no clock reads (`now` argument); lint header like other core crates; register `("ingest", Class::Layer(2))` in `TABLE` and add `ingest` to `WASM_CRATES`; add to `[workspace.dependencies]`.

## Out of scope
- `AgentUpdate`/`Archived`/`Deleted`/`Stream`/`Stats`/`Timeline`/`TimelineReset` reducers and activity derivation → sprint-006/task-001/002/003/005 (their event variants arrive with them).
- `UiRequest`/`UiResolved`/`RequestResult` reducers and `from_wire` classification, `Notify` effects → sprint-007/task-001.
- Observability reducers → sprint-010.
- Engine integration (`feed.ingest`) → task-002; effect execution → task-002.
- Client event emission and wire → model mapping → sprint-004/task-002, task-005.

## Acceptance criteria
- [ ] Given a slice at `connEpoch` 3, when an event tagged epoch 2 arrives, then the state is unchanged and no effect is returned; an event tagged 3 is applied; a `ConnState` tagged 4 raises the slice epoch. (live-state AC, D38)
- [ ] Given a daemon disconnect (`ConnState` backoff), then its agents are `stale` and remain in `card_order`; a later `Snapshot` clears `stale_since` and replaces agents and dialogs in one step with no intermediate empty slice. (D25)
- [ ] Given a `Snapshot` omitting a previously pending dialog, then it is gone afterwards; given one pending dialog returned in `ui_pending` plus the same dialog delivered twice, then exactly one item exists (upsert by key). (D8, D25, D36 — reducer half)
- [ ] Given two entries whose `Hello` carries the same `server_id`, then agents are counted once (the later entry is `duplicate_of` the first); given a changed `server_id` at one entry, then the old agents are removed. (D19)
- [ ] Given a `Snapshot` while a drill target exists on that server: target present → `refetching` + one `FetchTimeline`; target missing → `CloseDrill("agent gone")`.
- [ ] Given a `Snapshot` with running agents or agents lacking usage, then `FetchStats` effects are returned for exactly those.
- [ ] Given mixed statuses, one agent with a pending dialog and an errored agent, then `card_order` is attention → running → error → idle → closed with the documented tie-breaks; archived agents and duplicate-daemon agents are absent. (live-state AC)
- [ ] Given 3 of 5 agents with usage, then `usage_totals` reports `covered = 3`, `of = 5`; token and cost sums follow the spec.
- [ ] Given events for an unknown `entry_id`, then they are ignored and counted and never create state.
- [ ] Property test: for arbitrary sequences of the supported events, `apply` never panics, is deterministic (same input ⇒ same output), all ring bounds hold and `card_order` is a total order with no duplicates.
- [ ] `cargo xtask wasm` and `cargo xtask layers` pass with `ingest` registered as L2.

## Test / verification plan
- Build: `cargo build -p pistudiodashboard-ingest -p pistudiodashboard-model`.
- Tests: unit tests per reducer in `crates/ingest/src/*`, plus `crates/ingest/tests/properties.rs` using `proptest` — **new dev-dependency**: check licence (MIT OR Apache-2.0) and record it in the task summary (rule 9; confirm `proptest`'s transitive tree is permissive, e.g. via `cargo tree -p pistudiodashboard-ingest -e normal,dev` and a licence read). If proptest is rejected, use a seeded hand-rolled generator.
- Focused: `cargo test -p pistudiodashboard-ingest -p pistudiodashboard-model`.
- Gate: `cargo xtask ci` passes.

## Notes
- Decision: `DaemonEvent`, `DaemonEventKind`, `Effect` live in `model` (outline allows `model` or `protocol`; the layer map has `client` depend on `model` and L0 stay wire-only).
- Clock: reducers take `now`; the engine passes the event's own `at` (no `SystemTime` in L5 — it would not run on wasm32).
- TODO(verify) (live-state.md): none specific; the `cancelled` reason and other dialog items belong to sprint-007.
- Keep per-event cost O(1)/O(log n): 5,000 agents × token deltas later (live-state § Memory and scale) — avoid whole-map clones per event; apply a batch on one working copy.
