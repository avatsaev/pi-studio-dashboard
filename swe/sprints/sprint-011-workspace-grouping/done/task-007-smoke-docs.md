# Task 007 — Smoke against pi-studio, screenshots and docs

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Status:** done
- **Type:** test
- **Area:** cross-cutting (docs, smoke)
- **Priority:** P2
- **Estimated size:** XS
- **Depends on:** task-006

## Goal
Prove workspace grouping end to end against the real pi-studio dev daemon, capture screenshots, and document the feature.

## Context / why
The user's smoke harness is `../pi-studio` (`swe/notes/smoke-harness.md`); the user asked for screenshots of results. Docs keep the control protocol and README honest about the new command.

## Scope references
- `swe/specs/features/agent-workspace-grouping.md` § Acceptance criteria
- `swe/notes/smoke-harness.md`
- `docs/control-protocol.md`, `README.md` (feature list, if it lists monitor features), `swe/screenshots/`

## What to build
- Smoke: on the dev daemon (`127.0.0.1:6790`), create agents in at least three workspaces and one shared basename (`mkdir -p /tmp/smoke-ws/a/app /tmp/smoke-ws/b/app /tmp/smoke-ws/solo`; `$PS run "hello" --cwd <dir> -p mock/mock`), plus one `#ui confirm` agent in one workspace. Drive the app (or `pistudiodashboard-cli` + control protocol) through `view.setGroupBy` none → workspace → none; check labels `a/app`, `b/app`, `solo`, header counts, the attention group first; restart the app and confirm the mode was restored.
- Screenshots: grouped and flat, dark and light, into `swe/screenshots/` (`06-grouped-dark.png`, `06-grouped-light.png`, `06b-flat-workspace-dark.png`, `06b-flat-workspace-light.png`), via `npm run snapshot` with a sample mirroring the smoke data when no display is available — say which.
- Docs: confirm `docs/control-protocol.md` lists `view.setGroupBy` and the `groupBy` `ui.set` field (from tasks 003–004); add the feature to `README.md` only if it has a features list.

## Out of scope
- Any code change beyond fixes for defects the smoke finds (each fix with a test, golden rule 8).

## Acceptance criteria
- [ ] Every smoke step and its observed result recorded in the summary under "Smoke against pi-studio"; anything not runnable is stated with the reason, never marked passed.
- [ ] Four screenshots exist and were looked at.
- [ ] The spec's acceptance criteria are each ticked or explicitly carried to TODO(verify) in the summary.
- [ ] `cargo xtask ci` green.

## Test / verification plan
- Commands from `swe/notes/smoke-harness.md`; `cargo xtask ci`.

## Notes
- Never touch a real daemon home (`~/.pi-studio`); use `/tmp/pi-studio-dev` and `/tmp/pi-studio-cli` only.
