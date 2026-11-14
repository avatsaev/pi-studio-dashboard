# Task 001 — Workspace derivations: normalise, label, group order

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Status:** done
- **Type:** feature
- **Area:** crates/ingest (L2)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** none (builds on sprint-006, done)

## Goal
Pure L2 functions that turn `AgentEntry.cwd` into a normalised workspace, a display label, and a group-ordered card list, beside the existing `card_order`.

## Context / why
The grid cannot show or group by workspace until there is one deterministic, tested definition of "workspace", "label" and "group order". Keeping it in L2 makes it pure, wasm-safe and reusable by the engine view (task-003) without the frontend parsing paths (golden rule 1).

## Scope references
- `swe/specs/features/agent-workspace-grouping.md` § Decisions (W1, W4, W5, W7), § Normalising the workspace, § Label rules, § Grouping and ordering
- `swe/specs/architecture/live-state.md` § Derivations (`grouped_order` line)
- `crates/ingest/src/derive.rs` (`scope`, `rank`, `sort_key`, `card_order` live here; reuse, do not duplicate)
- `crates/ingest/src/lib.rs` (exports)
- `crates/model/src/agent.rs` (`AgentEntry.cwd: Option<String>`, already capped at 512 bytes by the protocol crate)

## What to build
- `workspace_of(cwd: Option<&str>) -> Option<String>`: trim; empty → `None`; strip trailing `/` and `\` while length > 1 (`/a/b/` == `/a/b`, `/` stays `/`); case-sensitive; no `..`/symlink resolution.
- Label derivation over a set of workspace paths (the groups/cards in the current filtered scope): base name = last non-empty segment split on `/` and `\` (`/` → `/`); equal base names are disambiguated by prepending parent segments until unique among those paths (`/work/a/app`, `/work/b/app` → `a/app`, `b/app`); identical paths on two daemons keep identical labels; `None` → `"No workspace"`; label capped at 80 characters on a char boundary.
  Suggested shape: `workspace_labels(state, filters) -> BTreeMap<(String /*serverId*/, Option<String> /*workspace*/), String>` so both flat cards and group headers read the same labels.
- `grouped_order(state, filters) -> Vec<WorkspaceGroup>` with `WorkspaceGroup { server_id, workspace: Option<String>, label, cards: Vec<(String, String)> }`:
  - same scope as `card_order` (not archived, daemon known and not duplicate, daemon + status filters applied);
  - bucket by `(server_id, workspace_of(cwd))`; cards inside a group sorted by the existing `sort_key`;
  - groups sorted by `(workspace.is_none(), best rank in group, daemon config position, label, workspace path)` — "No workspace" groups after every real workspace (W4), then W5; a total order;
  - no empty groups.
- Export from `crates/ingest/src/lib.rs`. No new dependency.

## Out of scope
- View serialisation, windowing, `groups.layout` (task-003); persistence (task-002); UI (tasks 005–006).

## Acceptance criteria
- [ ] `workspace_of`: `None`, `""`, `"   "` → `None`; `"/a/b/"` and `"/a/b"` → `"/a/b"`; `"/"` → `"/"`; `"C:\\w\\app\\"` → `"C:\\w\\app"`.
- [ ] Labels: `/home/u/proj/app` → `app`; `/work/a/app` + `/work/b/app` → `a/app`, `b/app`; a third `/x/a/app` disambiguates further (`work/a/app`, `x/a/app`, `b/app`); `None` → `No workspace`; a 1 MB/HTML path yields a label ≤ 80 chars without panicking.
- [ ] Group order: given groups whose best cards are attention / running / idle-only plus a No-workspace group with a running agent, the order is attention, running, idle-only, No workspace.
- [ ] Concatenating `grouped_order` cards restricted to one group equals `card_order` restricted to the same agents (inner order unchanged).
- [ ] The same path on two daemons yields two groups, ordered by daemon config position on ties.
- [ ] Daemon/status filters remove agents before grouping; a group with no remaining agents does not appear.
- [ ] Property test (proptest, already a dev-dependency): for arbitrary agent sets, `grouped_order` is deterministic regardless of insertion order, contains every scoped card exactly once, and never panics.
- [ ] Streaming-only changes (activity, usage) never change `grouped_order`.

## Test / verification plan
- Unit + property tests in `crates/ingest/src/derive.rs` (or a new `crates/ingest/src/workspace.rs` with its own tests).
- `cargo test -p pistudiodashboard-ingest`
- `cargo clippy -p pistudiodashboard-ingest --all-targets -- -D warnings`
- `cargo xtask ci` before moving to done (wasm step must stay green: no fs, no std::path).

## Notes
- Do not use `std::path::Path` for splitting: behaviour must be identical on Linux, macOS and wasm; split on both separators by hand.
- TODO(verify) (spec): whether a No-workspace group holding a pending dialog should still sort last — implement as written (last); the leading sort key is one place to change.
