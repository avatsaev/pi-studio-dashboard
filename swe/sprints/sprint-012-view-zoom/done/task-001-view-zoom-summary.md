# Task 001 — View zoom — Summary

- **Sprint:** sprints/sprint-012-view-zoom
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
- `settings.zoom` (config: `ZOOM_MIN/MAX/DEFAULT`, lenient parse with one issue for a bad value) and `Session::set_zoom` (saves only on change).
- Shell: `UiState.zoom` (default 100; `UiState` got a manual `Default`), `zoom.rs` (steps 50 67 75 80 90 100 110 125 150 175 200 250 300), `fields::zoom` for `ui.set`, commands `view.zoomIn` / `view.zoomOut` / `view.zoomReset` in the View menu with `Cmd+=`, `Cmd+-`, `Cmd+0` and reasons at the ends; `AppShell::set_zoom`, `adopt_settings` seeds it.
- App: `src-tauri/src/zoom.rs`; the owner's view sink applies `ui.zoom` to the main window with `WebviewWindow::set_zoom` when it changes, and `on_page_load(Finished)` re-applies the last level. No new webview permission.
- Frontend: `UiState.zoom` type; key matching treats `+` with Shift as the `=` key (zoom in).
- Docs/spec: `swe/specs/features/view-zoom.md`, config spec, overview index, `docs/control-protocol.md`, PLAN (sprint 012).

## Build & test results
```
$ cargo test (config 19, engine 51, shell 33, app 32 incl. saved zoom restored and saved) -> passed
$ npm run check && npm run lint && npm test -> clean, 233 tests passed
$ cargo xtask ci -> see end of this file
```

## Snapshots, looked at
`done/screenshots/view-menu-{dark,light}.png`: the View menu with Zoom In / Zoom Out / Actual Size and their shortcuts (`Ctrl+=`, `Ctrl+-`, `Ctrl+0`); at 125% all three enabled, at 300% Zoom In greyed (its reason is the tooltip). The snapshot browser cannot show the zoom itself (it is native webview zoom).

## Not verified
The actual scaling in WebKitGTK / WKWebView / WebView2: there is no display here, so `set_zoom` was compiled and its decision logic unit-tested, but not seen. Run the app and press Ctrl+= to confirm.
