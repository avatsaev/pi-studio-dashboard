# Task 007 — Smoke against pi-studio, screenshots, docs — Summary

- **Sprint:** sprints/sprint-011-workspace-grouping
- **Completed:** 2026-11-14
- **Status:** done

## What was done
Smoke of the whole feature against the real dev daemon, four real-data screenshots, docs aligned. During the sprint the user decided that **grouping by workspace is the default** (`GroupBy::Workspace`; a config file without the key, a new install and an unknown value all start grouped; an explicit `none` is saved and restored). Code, tests, spec (W2, W3, state, acceptance) and docs were updated for it. `document.inspect` called without `groupBy` still returns the flat list (a bare query); the app always passes its `ui.groupBy`.

## Smoke against pi-studio
Dev daemon `127.0.0.1:6790` (running). Agents: `$PS run "hello" --cwd /tmp/smoke-ws/{a/app,b/app,solo}`, `#ui confirm` in `smoke-ws/b/app`, `#stream fail` in `smoke-ws/solo` (the daemon also still held agents from earlier smoke runs in `/tmp` and `/tmp/gb-smoke/*`). App: `target/debug/pistudiodashboard --headless --control 7878`, throwaway `PISTUDIODASHBOARD_CONFIG_DIR=/tmp/gb-cfg`, driven over the control socket.
- Fresh config dir, first start: `ui.groupBy: workspace`, `document.groups` = `gb-smoke/solo` (2, attention), `smoke-ws/b/app` (2, attention), `smoke-ws/solo` (2, attention), `tmp` (16, attention), `gb-smoke/a/app` (1), … ; View menu: `Group: By Workspace` checked.
- `view.setGroupBy none` → `ui.groupBy: none`, no `groups`, menu check moved; cards carried `cwd`/`workspaceLabel` (`smoke-ws/solo`, `smoke-ws/b/app`, `smoke-ws/a/app`: parents added because `gb-smoke/...` share the folder names `solo`, `app`).
- After the 250 ms save debounce `config.json` held `"groupBy": "none"`; a restarted app came up `ui.groupBy: none` (flat, menu check on `Group: None`). Earlier in the sprint the reverse (set `workspace` under the old default, restart, restored) was also observed.
- `groupBy: "banana"` through `engine.execute` → `` `groupBy` is one of: none, workspace ``; through `document.inspect` → `bad params: ... got `banana``.
- Not covered: a native window (no display); the platform webview; macOS.

## Screenshots (looked at)
`swe/screenshots/06-grouped-{dark,light}.png` (real view from the daemon, grouped: headers with path, count, "needs attention", cards without the workspace line) and `06b-flat-workspace-{dark,light}.png` (flat: workspace after the daemon on each card, e.g. `Local · smoke-ws/b/app · mock`). The shots are `npm run snapshot --view <ui.inspect JSON from the running app>`, so the data is the real engine view of real daemon data, drawn by the real frontend in headless Chromium. Sprint-local shots: `done/screenshots/005-*`, `006-*`.

## Docs
`docs/control-protocol.md` (`groupBy` in `ui.set`, `document.inspect`, `view.setGroupBy`, card fields, `groups`, default), `docs/development.md` (`--agents`, `--scroll-to-slice`), spec files. README has no feature list, so no change.

## Acceptance criteria (spec)
- [x] flat shows workspace + path tooltip; grouped sections, counts, No workspace last (unit + smoke)
- [x] `/a/b` vs `/a/b/` one group; same path on two daemons two groups (ingest tests)
- [x] attention group first, idle after; streaming deltas do not move groups (ingest test is weak for deltas, see task 001; ordering depends only on status/attention by construction)
- [x] filters drop groups; attention rail unchanged (view tests)
- [x] 500 agents / 200 workspaces: ≤ 200 cards, one layout entry per group, exact scroll geometry (engine + frontend tests, snapshot)
- [x] hostile cwd plain text (engine + header tests)
- [x] bad value refused; `ui.set`, menu, command reach the same state
- [x] persistence: default grouped; `none` saved and restored; unknown value → grouped + issue (config + app tests, smoke)
- [x] snapshots both themes looked at
- [x] `cargo xtask ci` green

## Follow-ups / TODO(verify)
Carried from the spec: a shortcut for `view.setGroupBy` (none), "No workspace" group with a pending dialog sorts last (as written), whether daemons ever change an agent's `cwd`.
