# Task 002 — Real-daemon acceptance pass

- **Sprint:** sprints/sprint-009-stage1-hardening
- **Status:** backlog
- **Type:** test
- **Area:** cross-cutting
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-001

## Goal
Run the stage 1 acceptance scenarios against a real local pi-studio dev daemon and record the results, re-vendoring fixtures if pi-studio moved.

## Context / why
Fake-daemon tests prove our reading of the spec; only a real daemon proves the contract (brief acceptance 1–14 as carried by the feature specs). Vendored fixtures were taken at pi-studio commit `47d80e6` (0.0.115).

## Scope references
- `swe/grills/daemon-monitoring-dashboard.md` § Acceptance criteria (1–14)
- `swe/specs/features/daemon-connections.md` § Acceptance criteria
- `swe/specs/features/agent-monitor.md` § Acceptance criteria
- `swe/specs/features/attention-and-interventions.md` § Acceptance criteria
- `swe/specs/features/notifications-and-tray.md` § Acceptance criteria
- `crates/protocol/tests/fixtures/README.md` (re-vendor procedure), `crates/protocol/tests/fixtures/pi-studio/SOURCE.json`
- `../pi-studio/package.json` (script `dev:daemon`), `../pi-studio/README.md` § Configuration (env vars)
- `swe/notes/` (acceptance record, e.g. `stage1-acceptance.md`)

## What to build
Procedure (record every step's observed result; do not paste secrets):
1. Check `../pi-studio` HEAD against `SOURCE.json`. If it moved, follow the re-vendor procedure, run `cargo test -p pistudiodashboard-protocol`, and fix decode drift before continuing.
2. Start the dev daemon (it uses the **mock** provider; there is no `npm run dev` in pi-studio — the real script is `dev:daemon`): in `../pi-studio` run
   `PI_STUDIO_HOME=/tmp/pi-studio-dev PI_STUDIO_LISTEN=127.0.0.1:6790 npm run dev:daemon` (builds the server then runs `packages/server/dist/daemon/dev-main.js`; optional `PI_STUDIO_MOCK_TURN_DELAY_MS`, `PI_STUDIO_SERVER_ID`, `PI_STUDIO_PASSWORD` for the wrong-password case). Create agents with the pi-studio CLI/web UI against `--host 127.0.0.1:6790` using the `mock` provider.
3. Launch the dashboard (`cargo xtask run` / the dev workflow in `docs/development.md`) with its own temporary config dir; add the daemon entry (or let `PI_STUDIO_LISTEN` auto-detect do it).
4. Walk the checklist, recording pass/fail and notes: connect + cards appear; status/activity changes live; usage totals; filters; drill-down; a `confirm` and a `select` dialog answered from the dashboard (and one answered in the web UI first → "answered elsewhere"); editor/input hand-off opens the web UI with the expected URL; interrupt of a running mock turn with a pending dialog staying pending; notification and click-to-focus with the window hidden; wrong password → `auth_failed` message and no retry loop; kill and restart the daemon → stale then recover without user action (`connEpoch` replacement, no stale cards remain); remove daemon.
5. Write results to `swe/notes/stage1-acceptance.md` (date, pi-studio commit, daemon version, per-criterion result, defects found). Defects become new backlog tasks or are fixed here only if trivial; never silently skipped.
Whether the mock provider can raise dialogs is unknown — if it cannot, record which dialog criteria were verified instead against the vendored fixtures plus the fake daemon and list them as "not verified on real daemon".

## Out of scope
- Load/idle budgets — sprint-009/task-001.
- Docs — sprint-009/task-003.
- Stage 2 (health/logs) acceptance — covered by sprint-010 snapshot/test tasks; real-daemon pass for them is run against the same dev daemon only if `daemonObservability` is advertised (TODO(verify)).

## Acceptance criteria
- [ ] Given the dev daemon started with `npm run dev:daemon` and the dashboard connected, then the checklist in `swe/notes/stage1-acceptance.md` has a recorded result for every stage 1 acceptance criterion (pass, fail with defect link, or explicitly not verifiable with the reason).
- [ ] Given pi-studio HEAD differs from `SOURCE.json`, then fixtures were re-vendored and `SOURCE.json` updated, and tests pass; otherwise the record states they are current.
- [ ] Given a wrong password, then the chip shows the 401 message and no reconnect loop occurs.
- [ ] Given the daemon killed and restarted, then the dashboard recovers without user action and no stale card persists.
- [ ] Given the pass is complete, then no password or dialog text appears in the dashboard's log file (grep the log and record the result).

## Test / verification plan
- No new code is required unless defects are fixed; run `cargo xtask ci` at the end.
- Record exact commands and observations in `swe/notes/stage1-acceptance.md`.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- Correction to the outline: pi-studio has no `npm run dev`; use `npm run dev:daemon` (README: `PI_STUDIO_HOME`, `PI_STUDIO_LISTEN=127.0.0.1:6790` example).
- TODO(verify): whether the dev daemon advertises `daemonObservability` and whether the mock provider can emit dialogs; record the answer.
