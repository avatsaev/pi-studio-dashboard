# Task 001 — View zoom: commands, saved setting, native webview zoom, shortcuts

- **Sprint:** sprints/sprint-012-view-zoom
- **Status:** done
- **Type:** feature
- **Area:** crates/config (L3), crates/engine (L5), crates/shell (L6), apps/pistudiodashboard/src-tauri, apps/pistudiodashboard/src
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** none

## Goal
View menu entries Zoom In, Zoom Out and Actual Size with keyboard shortcuts; the level is saved and applied to the window.

## Scope references
- `swe/specs/features/view-zoom.md` (Z1–Z5), `swe/specs/architecture/config-and-secrets.md` (`settings.zoom`)
- `crates/shell/src/{menus,state,fields,control,lib,zoom}.rs`, `crates/config/src/{model,parse}.rs`, `crates/engine/src/daemon_cmds.rs`, `apps/pistudiodashboard/src-tauri/src/{main,zoom}.rs`, `apps/pistudiodashboard/src/lib/shortcuts.ts`

## Acceptance criteria
- [x] `view.zoomIn` / `zoomOut` / `zoomReset` step through 50…300 and save each change; disabled at the ends with a reason
- [x] View menu entries with `Cmd+=`, `Cmd+-`, `Cmd+0`; `+` (Shift) also zooms in
- [x] `ui.set {zoom}` validated (50–300 whole percent) and saved; `ui.inspect` returns `ui.zoom`
- [x] Saved zoom restored at startup; invalid config value → 100 + issue
- [x] The app applies the level to the webview (`set_zoom`), also after a page load
- [ ] Visible effect in a real platform webview (no display here)

## Test / verification plan
`cargo test` (config, engine, shell, app), `npm run check && npm run lint && npm test`, `npm run snapshot -- --click View`, `cargo xtask ci`.
