# Task 003 — Model crate: `MonitorState` and its bounds

- **Sprint:** sprints/sprint-001-core-crates
- **Status:** done
- **Type:** feature
- **Area:** L1 `crates/model` (live state data)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-001

## Goal
Create `pistudiodashboard-model` (L1): the plain-serde `MonitorState` and its component types, a bounded ring helper, every cap from the live-state spec, and `validate()` — not yet used by the engine.

## Context / why
`MonitorState` is the one in-memory source of truth (daemons, agents, dialogs, drill-down, observability, settings mirror). Putting the data and every bound in an L1 crate first lets the ingest reducers (sprint-002/task-001 onward) and the engine be written against stable types, with invariants enforced in the model (same rules as the scaffold's `crates/doc`: plain serde, typed ids, caps, `validate()`, `#[serde(default)]` on later-added fields).

## Scope references
- `swe/specs/architecture/live-state.md` § `MonitorState` (field table), § Bounds and enforcement, § Dialog classification (`from_wire` shape only), § Memory and scale
- `swe/specs/architecture/app-structure.md` § Layer / crate map (L1 row)
- `swe/specs/architecture/config-and-secrets.md` § File format (settings mirror shape)
- `swe/grills/daemon-monitoring-dashboard.md` D8, D15–D17, D19, D25, D28, D36, D38
- Template reference for style: `crates/doc/src/lib.rs` (caps, `Invalid` error enum, `validate()`, tests) — this crate replaces it in sprint-002/task-002
- Create `crates/model/{Cargo.toml,src/lib.rs,…}`; register in `xtask/src/layers.rs` `TABLE` as `("model", Class::Layer(1))`; add to `[workspace.dependencies]`

## What to build
- Crate `pistudiodashboard-model`, depends only on `pistudiodashboard-protocol` (L0; e.g. for `WireTime` helpers if needed — do not add a dependency just to have one) plus `serde`, `serde_json`, `thiserror`. Same lint headers as other core crates. No I/O, no clock reads (`now` is always an argument), wasm-clean.
- Types (all `Serialize + Deserialize + Clone + Debug + PartialEq`, `#[serde(default, rename_all = "camelCase")]`, fields exactly as the live-state table; `Option` where the spec says `?`):
  - `MonitorState { daemons: Vec<DaemonSlice> /*config order, keyed by entry_id*/, agents: BTreeMap/ordered map keyed (server_id, agent_id), ui_requests: map keyed (server_id, request_id), drilldown: Option<DrillBuffer>, observability: Observability /*empty placeholder struct owned by sprint-010*/, settings: SettingsMirror }`. Use a serialisable key strategy (e.g. a `String` key `"<serverId>\u{1f}<agentId>"` or a `Vec` of entries) — maps with tuple keys do not serialise to JSON; document the choice and keep lookup O(log n) or O(1).
  - `DaemonSlice { entry_id, name, conn: ConnState, conn_epoch: u64, server_id: Option<String>, info: Option<ServerInfoView{hostname?, version?, features}>, degraded: BTreeSet<DegradedReason>, duplicate_of: Option<String>, health: Option<Health{reachable, at}>, stale_since: Option<i64>, last_error: Option<ConnErrorView>, counters: DaemonCounters{unknown, malformed, …}, events: Ring<EventRecord> /*cap 500*/ }`.
  - `ConnState` (idle | probing | connecting | handshaking | syncing | live | backoff | auth_failed | host_rejected | disabled), `DegradedReason { NoObservability }` — **owned here**, so the L4 client (sprint-004/task-002) and ingest share them.
  - `AgentStatus { Initializing, Idle, Running, Error, Closed, Unknown }` with lenient `from_wire(&str)`; `AgentEntry`, `ActivitySummary { current_tool, last_output_line, turn_state: None|Started|Completed|Failed|Canceled, last_error_message }`, `UsageSnapshot { None | Some{ tokens?, cost?, context_usage?, taken_at } }`, `Attention { pending_dialogs, errored }`, `stale`.
  - `UiRequestItem` with `RequestKind { Confirm, Select, Input, Editor, Other }`, `Answerability { Answerable, HandOff }`, title/message/options fields, `options_truncated`, `timeout_ms`, `created_at`, `expires_at`, `raised_at`, `in_flight`, `in_flight_response`. Constructor `from_wire` is sprint-007/task-001; this task only defines the struct and the cap constants.
  - `DrillBuffer { target: (server_id, agent_id), items: Ring<DrillItem> /*cap 2000*/, seq_high: Option<u64>, state: Loading|Live|Refetching, truncated_at_start, stale }`, `DrillItem` enum as in the spec.
  - `EventRecord { at, kind: DialogRaised|DialogResolved|TurnFailed|AgentError|ConnState|AgentRemoved, agent_id?, summary }` — `summary` is built only from fixed vocab (never option text, message, command or path content; D29, D32).
  - `SettingsMirror { theme: dark|light|system, notifications{needs_input, failures}, close_to_tray, attention_rail_collapsed }` with the config defaults (`theme: system`? — mirror `config` defaults: `system`, both notifications on, `closeToTray` true; the mirror is filled from `Config` by the engine later).
- `pub struct Ring<T>` (bounded FIFO; `push` drops the oldest at capacity and returns whether it dropped; `len`, `iter`, `capacity`, serde as a sequence, deserialisation truncates an over-long input to the newest `cap` items) used by `events` (500) and `DrillBuffer.items` (2,000).
- Caps as `pub const`s with the spec's values: `EVENT_RING = 500`, `DRILL_ITEMS = 2000`, `DRILL_HOLD = 2000`, `DIALOG_TITLE = 200`, `DIALOG_MESSAGE = 500`, `DIALOG_OPTION = 120`, `DIALOG_OPTIONS = 50`, `SUMMARY_TITLE = 200`, `LABEL = 120`, `OUTPUT_LINE = 200`, `ERROR_TEXT = 200`, `ASSISTANT_TAIL = 1024`, `TOOL_OUTPUT_TAIL_LINES = 12`, `TOOL_OUTPUT_LINE = 500`.
- `MonitorState::validate(&self) -> Result<(), Invalid>`: ring lengths ≤ caps; every string cap above; `ui_requests`/`agents` keys consistent with their entries; unique `entry_id`s; `drilldown.target` points at an existing agent or is allowed (stale) — decide and document; typed `Invalid` enum with `Display` messages.
- Helpers `cap_string(&str, max) -> String` reuse the L0 helper from task-001 if the model depends on protocol; otherwise define locally once.

## Out of scope
- Reducers, `DaemonEvent`, `Effect`, derivations → sprint-002/task-001 (and sprint-006/007 for the per-event reducers).
- `UiRequestItem::from_wire` classification → sprint-007/task-001; `AgentEntry::from_list_item` → sprint-002/task-001 (uses model-owned list-item input) and sprint-006.
- `Observability` contents (health/logs buffers) → sprint-010/task-002; here an empty serde struct.
- Wiring into `Session`/`document.inspect` → sprint-002/task-002.
- Deleting `crates/doc` (sample domain) → sprint-002/task-002.

## Acceptance criteria
- [ ] Given a default `MonitorState`, when serialised and deserialised, then it round-trips and `validate()` is `Ok`.
- [ ] Given a state with a populated daemon, agent, dialog item, drill buffer and settings, then it round-trips through `serde_json` with camelCase keys and map keys that survive JSON.
- [ ] Given 1,000 pushes into `Ring::new(500)`, then it holds the latest 500 in order, and `push` reports a drop once full. (live-state AC "ring holds the latest 500")
- [ ] Given 5,000 pushes into the drill ring (cap 2,000), then `len() == 2000` and the newest item is last. (live-state AC "5,000 stream items")
- [ ] Given a JSON document whose ring has more than its cap, when deserialised, then it is truncated to the newest `cap` items (hostile input cannot exceed bounds).
- [ ] Given an `AgentStatus` wire string it does not know (and a missing field), then `from_wire` yields `Unknown` and deserialising an agent without optional fields succeeds (`#[serde(default)]`).
- [ ] Given a state violating a cap (oversized dialog title, duplicate `entry_id`, ring over capacity constructed by hand), then `validate()` returns the matching `Invalid` variant.
- [ ] `cargo xtask wasm` builds `pistudiodashboard-model`; `cargo xtask layers` accepts it as L1 with only `protocol` as a workspace dependency.

## Test / verification plan
- Build: `cargo build -p pistudiodashboard-model`.
- Tests: unit tests in the crate (ring bounds, serde round-trip, validate cases, status leniency); keep tests few and consumer-visible.
- Focused: `cargo test -p pistudiodashboard-model`.
- Gate: `cargo xtask ci` passes (the sample `doc`/`ops`/`format`/`engine` still build untouched).

## Notes
- Inherited TODO(verify) (live-state.md): none block this task; the `cancelled` resolved reason is a reducer concern.
- Decision recorded here: `ConnState` and `DegradedReason` live in `model` (protocol-client.md says "defined by protocol-client"; the layer map has `client` depending on `model`, so the shared definition belongs below both). Sprint-004 reuses them.
- Memory budget (live-state § Memory and scale): `AgentEntry` ≈ ≤ 2 KiB — keep strings `String` (no `Arc` games) and avoid per-agent `Vec`s beyond `labels`.
- `serde` feature `rc` is enabled workspace-wide; do not use `Rc`/`Arc` inside serialisable state.
