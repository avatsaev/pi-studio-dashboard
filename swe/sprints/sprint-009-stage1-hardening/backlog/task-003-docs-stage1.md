# Task 003 — Stage 1 documentation

- **Sprint:** sprints/sprint-009-stage1-hardening
- **Status:** backlog
- **Type:** docs
- **Area:** docs / README / AGENTS
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Bring README, development docs, control-protocol docs and AGENTS.md in line with the finished stage 1 app and reconcile the open TODO(verify) list.

## Context / why
Docs must describe the shipped behavior (every command, run instructions, daemon setup) so the next contributor needs nothing else. Open verification items from the specs need an owner or a resolution.

## Scope references
- `swe/specs/overview.md`
- `README.md`, `docs/development.md`, `docs/control-protocol.md`, `docs/architecture.md`, `AGENTS.md`, `docs/contributing.md`
- `swe/notes/stage1-acceptance.md` (from task-002)
- Every spec's `TODO(verify)` section under `swe/specs/`

## What to build
- `README.md`: what the app is, how to run it (dev and built), how to add a daemon (host, port, password via keychain, web UI URL, local auto-detect), the two permitted interventions and the read-only guarantee, tray/notification notes per platform.
- `docs/development.md`: workspace commands, how to run against a pi-studio dev daemon (`npm run dev:daemon` with `PI_STUDIO_HOME`/`PI_STUDIO_LISTEN` as in task-002), how to re-vendor fixtures, load/idle measurement procedure from task-001.
- `docs/control-protocol.md`: every registered command with params, results and disabled reasons (generated or checked against the registry so it cannot drift; add a test only if the repo already has such a docs-vs-registry check).
- `AGENTS.md`: workspace map and crate/layer table updated (protocol, model, reducers, config, client, engine, shell), golden rules unchanged.
- Reconcile the `TODO(verify)` items across all specs: for each, mark resolved (with the evidence from acceptance), carry forward as a tracked open question in `swe/PLAN.md` open questions, or turn into a backlog task. Do not delete an item without a resolution recorded.

## Out of scope
- Stage 2 documentation (health/logs commands) — added by the sprint-010 tasks as they land.
- Any new code.

## Acceptance criteria
- [ ] Given a fresh clone, when a new contributor follows README and `docs/development.md`, then they can build, run against a dev daemon and run the gate without extra knowledge.
- [ ] Given `docs/control-protocol.md`, then every command in the registry is documented and no removed sample-domain command remains.
- [ ] Given `AGENTS.md`, then the workspace map matches `xtask/src/layers.rs`.
- [ ] Given the spec `TODO(verify)` items, then each has a recorded resolution, open-question entry or backlog task.

## Test / verification plan
- Check links and command names by running the documented commands once.
- `cargo xtask ci` passes.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- Docs-only task; no `npm run snapshot` needed.
