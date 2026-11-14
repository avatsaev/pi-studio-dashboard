# Feature — View Zoom (Zoom In, Zoom Out, Actual Size)

> Part of: [overview.md](../overview.md)
> Dependencies: [app-structure.md](../architecture/app-structure.md), [config-and-secrets.md](../architecture/config-and-secrets.md)
> Source: user request (2026-11-14): "add zoom in/out feature in the view menu, with shortcuts". Decisions Z1–Z5 are this spec's own.

## Purpose
Make the whole window's content larger or smaller, like a browser's page zoom, from the View menu and the keyboard.

## Decisions
| # | Decision |
|---|----------|
| Z1 | Zoom is the webview's native page zoom, applied by the app binary (`WebviewWindow::set_zoom`), not CSS scaling. The layout, and the card grid's window maths with it, then simply sees a smaller or larger viewport in CSS pixels. It needs no webview permission (golden rule 6). |
| Z2 | The level is `UiState.zoom`, a whole percent in `50..=300`, default 100, and is **saved** (`settings.zoom`) and restored at startup, like the grouping mode. |
| Z3 | Zoom In / Out move through the usual browser steps `50 67 75 80 90 100 110 125 150 175 200 250 300`; any in-range value set through `ui.set` still has a defined next step. |
| Z4 | Shortcuts: `Cmd+=` (also `Cmd+Shift+=`, i.e. `+`), `Cmd+-`, `Cmd+0` (`Cmd` is Ctrl off macOS). |
| Z5 | A command that cannot run says why: Zoom In "already at the largest size", Zoom Out "already at the smallest size", Actual Size "already at 100%". |

## Public contract
| Command id | Args | Effect | Enabled / reason |
|---|---|---|---|
| `view.zoomIn` | `{}` | `UiState.zoom` to the next larger step; saved | not at 300 |
| `view.zoomOut` | `{}` | next smaller step; saved | not at 50 |
| `view.zoomReset` | `{}` | 100; saved | not at 100 |

Shell commands in the View menu (labels `Zoom In`, `Zoom Out`, `Actual Size`). `ui.set {zoom}` accepts a whole percent in range (otherwise `` `zoom` is a whole percent from 50 to 300 ``) and saves it; `ui.inspect` returns `ui.zoom`. Config: `settings.zoom` (integer, absent or out of range → 100, with one `DefaultedValue` issue for an invalid value; the rest of the file loads).

## Behavior
```
on a view with ui.zoom != last applied: webview.set_zoom(zoom / 100)
on page load finished:                  webview.set_zoom(last seen zoom / 100)   # a fresh page starts at 100%
```
The frontend draws no zoom control and scales nothing; it only matches the shortcut keys it is sent (and treats `+` with Shift as the `=` key).

## Error handling & edge cases
| Condition | Expected behavior |
|---|---|
| `ui.set {zoom: 49 | 301 | "big" | 1.5 | null}` | refused naming the range; state unchanged, nothing saved |
| Config `zoom` out of range or not an integer | 100 + one issue |
| Newer-version (read-only) config | zoom changes in memory, not saved, the usual notice |
| Headless app | zoom is state only; no window |
| `set_zoom` fails | logged; the level in `UiState` is unchanged |

## Acceptance criteria
- [x] Zoom In/Out step through the levels and each change is saved; Actual Size returns to 100.
- [x] At 300 Zoom In is disabled with its reason (50 and 100 likewise).
- [x] The three entries are in the View menu with `Cmd+=`, `Cmd+-`, `Cmd+0`; `+` (Shift) zooms in.
- [x] `ui.set` validates the range; a saved zoom is restored at startup; a bad one defaults with an issue.
- [ ] TODO(verify): the visible effect in the platform webviews (WebKitGTK on Linux, WKWebView on macOS, WebView2) — no display here; the snapshot tool draws the menu but cannot zoom.

## TODO(verify)
- Whether platform webviews keep `set_zoom` across reloads without the page-load re-apply (kept anyway).
- Ctrl+mouse-wheel zoom is not offered.
