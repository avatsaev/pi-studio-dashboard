# Task 001 — Workspace derivations — Summary

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`crates/ingest/src/workspace.rs`: `workspace_of` (trim, drop trailing `/` `\`, `None` when empty), `workspace_labels` / `label_of` (base name, parent segments added only to separate paths; capped at 80 chars; `No workspace`), `grouped_order` → `WorkspaceGroup { server_id, workspace, label, cards, needs_attention }`. Groups sort by (no-workspace last, best card rank, daemon config position, label, path); cards inside use the existing `sort_key`. `derive.rs` helpers (`scope`, `rank`, `sort_key`, `needs_attention`) became `pub(crate)`; no duplication.

## Files created / changed
| File | Change |
|------|--------|
| `crates/ingest/src/workspace.rs` | created, 7 unit tests |
| `crates/ingest/tests/workspace.rs` | created: proptest (order-independent, complete, inner order = `card_order`), streaming-deltas test |
| `crates/ingest/src/derive.rs`, `src/lib.rs` | visibility, exports |

## How it satisfies the scope
Spec § Normalising, § Label rules, § Grouping and ordering as written. The label map is keyed by normalised path (same path on two daemons shares a label, as the spec says). No `std::path` use, wasm step green.

## Build & test results
```
$ cargo test -p pistudiodashboard-ingest          -> all passed (12 unit + integration, incl. proptest)
$ cargo clippy -p pistudiodashboard-ingest --all-targets -- -D warnings -> clean
$ cargo xtask ci                                  -> all 8 steps passed
```

## Acceptance criteria
- [x] `workspace_of` cases (unit test)
- [x] Labels incl. three-way disambiguation, No workspace, hostile path ≤ 80 chars
- [x] Group order attention / running / error / idle / No workspace
- [x] Inner order equals `card_order` (unit + property)
- [x] Same path on two daemons → two groups, config order
- [x] Filters before grouping, no empty groups
- [x] Property test: deterministic, complete, no panic
- [~] Streaming-only changes: tested with agents that have no cwd (one group); the order depends only on status/attention by construction.

## Follow-ups / TODO(verify)
- No-workspace group with a pending dialog sorts last (as spec'd); change the leading sort key if that is reversed.
