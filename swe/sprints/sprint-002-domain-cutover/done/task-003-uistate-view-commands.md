# Task 003 — `UiState` and the view commands

- **Sprint:** sprints/sprint-002-domain-cutover
- **Status:** done
- **Type:** feature
- **Area:** L6 `crates/shell` (`state.rs`, `control.rs`, `menus.rs`, `shortcuts.rs`, `tests.rs`)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Give the shell its final `UiState` (theme incl. `system`, filters, rail, focus, grid window, dialog, diagnostics target, toasts, status), an all-or-nothing validated `ui.set`, the `view.*` shell commands, and `MENU_ORDER = Daemon, Agent, View`.

## Context / why
Rule 5: what automation should see lives in `UiState`; the frontend keeps only on-screen widget state. These fields drive the card filters, rail and dialogs of every later UI task, and `document.inspect` (task-002) already accepts the same values as params. Committing the shape and validation now means the frontend types (task-004) and later commands (`daemon.add` opens a `daemonForm` dialog, etc.) have a fixed target.

## Scope references
- `swe/specs/architecture/app-structure.md` § UiState (field table and "Not in UiState" rationale), § Command registry (`view.*` rows; rules on `view.*` being shell commands, param-bound menu entries, `MENU_ORDER`), § Webview boundary
- `swe/specs/architecture/design-system.md` § Theme switching (the view carries the choice, not the resolution)
- `swe/specs/architecture/config-and-secrets.md` § Settings commands (theme validation, `invalid_theme`)
- `swe/grills/daemon-monitoring-dashboard.md` D14, D15, D21, D28
- Real files: `crates/shell/src/{state,control,menus,shortcuts,lib,tests}.rs`, `crates/shell/examples/view.rs`

## What to build
- **`state.rs`:** `ThemeKind { Dark, Light, System }` (default `Dark`, D21; `from_name` accepts the three, case-insensitive); `UiState` (serde `default`, `camelCase`) with exactly: `theme`, `filter_daemons: Vec<String>`, `filter_statuses: Vec<AgentStatus>` (the model's enum), `attention_rail_collapsed: bool`, `focus_item: Option<FocusItem{server_id, item_id}>`, `grid_window: GridWindow{start: u32, count: u32}` (default `{0, 100}`), `dialog: Option<Dialog>`, `diagnostics_target: Option<String>`, `toasts` (already added in task-002; bounded 5), `status`. `Dialog` is a tagged enum (`kind`): `daemonForm{mode: add|edit, entryId?}`, `password{entryId}`, `confirmInterrupt{serverId, agentId}`, `removeDaemon{entryId}`. Typed-but-unsubmitted text is **not** here.
- **`ui.set` (`control.rs`)** — validate every field, then apply all (a bad request changes nothing; unknown field or read-only field → error naming it):
  - `theme`: `dark|light|system` else error `` `theme` is `dark`, `light` or `system` ``;
  - `filterDaemons`: array of ≤ 50 strings, each 1–128 chars; `filterStatuses`: array of known status words (`initializing|idle|running|error|closed|unknown`), no duplicates;
  - `attentionRailCollapsed`: bool; `focusItem`: only `null` (clear) — setting a target is `window.focusItem` (sprint-008); `gridWindow`: `{start, count}` non-negative integers, `count` ≤ 1,000; `dialog`: `null` or a valid `Dialog` object with string ids ≤ 128 chars (open/close only — submitting goes through commands); `diagnosticsTarget`: `null` or a string ≤ 128 chars;
  - `toasts`, `status`: read-only → `` `toasts` is read-only ``.
- **View commands.** Extend `ShellCommand` to `{id, label, menu: &'static str, shortcut: Option<&'static str>, params: &'static str, run: fn(&mut AppShell, &Value) -> Result<(), String>, checked: …}` (they stay in the shell: no engine, no recording). Commands: `view.setTheme {theme}` (menu View), `view.filterDaemons {serverIds}` (no menu), `view.filterStatuses {statuses}` (no menu), `view.toggleAttentionRail {}` (menu View, `Cmd+Shift+A`), `view.setGridWindow {start,count}` (no menu). Params go through the same field validators as `ui.set` (one validation function per field, shared). They are listed by `engine.commands` (merge shell commands into the list the control protocol returns, flagged `layer: "shell"`) and runnable via `engine.execute` through `AppShell::run`.
- **Param-bound menu entries** (resolves the spec's TODO): `menus.rs` generates one menu entry per `theme` value (`Theme: Dark / Light / System`) with id `view.setTheme:dark|light|system`, a `checked` state and fixed params; `menus::invoke` splits `<command>:<value>` and runs the command with the bound params. Entries without a menu (`filter*`, `setGridWindow`) do not appear.
- **`MENU_ORDER = ["Daemon", "Agent", "View"]`**. `File`, `Edit`, `Item` are gone from the code. `Daemon` and `Agent` have no commands yet (they get entries in sprints 005–007); `menus::model` still returns all three in order.
- `shortcuts.rs`: ensure `Cmd+Shift+A` parses; the registry's shortcut-uniqueness test also covers shell commands.
- Persisting `theme` and `attentionRailCollapsed` through `config_store` is **not** done here (sprint-005/task-002); decision recorded: `UiState` is the runtime source of truth for theme and rail, `MonitorState.settings` mirrors notification/tray settings, and the config file is written from them. Document it in the `state.rs` module doc.
- `view()` (control.rs) passes `filterDaemons`, `filterStatuses`, `gridWindow`, `attentionRailCollapsed` from `UiState` to `document.inspect`; update `crates/shell/examples/view.rs` and the module docs for `ui.set`.

## Out of scope
- Frontend (`types.ts`, theme application, components) → task-004 and sprint-003.
- `window.focusItem`, `settings.*` commands, tray/close-to-tray → sprint-008.
- Commands that open dialogs (`daemon.add`, …) and diagnostics (`daemon.showDiagnostics`) → sprints 005–007; only the `dialog`/`diagnosticsTarget` state and `ui.set` for them live here.
- Persistence of theme/rail to the config file → sprint-005/task-002.
- `docs/control-protocol.md` field list → task-005.

## Acceptance criteria
- [ ] Given `ui.inspect`, then `ui` contains exactly `theme, filterDaemons, filterStatuses, attentionRailCollapsed, focusItem, gridWindow, dialog, diagnosticsTarget, toasts, status` with the defaults above, and `menus` are ordered `Daemon, Agent, View`.
- [ ] Given `ui.set {theme:"system"}` / `"light"` / `"dark"`, then `ui.inspect` shows that choice (the choice, never a resolution); given `{theme:"neon"}` or `{theme:"dark", bogus:1}` or `{filterStatuses:["nope"]}`, then the call errors and **no** field changed (all-or-nothing).
- [ ] Given `ui.set {toasts:[]}` or `{status:"x"}`, then it is refused as read-only; given `ui.set {focusItem:{…}}`, refused (clear-only); `{focusItem:null}` clears.
- [ ] Given `ui.set {dialog:{kind:"password", entryId:"e1"}}` then the dialog is open; `{dialog:null}` closes it; a dialog with an over-long id or unknown `kind` is refused.
- [ ] Given `view.filterDaemons {serverIds:["s1"]}`, `view.filterStatuses {statuses:["running"]}`, `view.setGridWindow {start:20,count:50}` and `view.toggleAttentionRail`, then the corresponding `UiState` fields change and a failure (e.g. `count: 100000`) reaches `ui.status` without changing state.
- [ ] Given the View menu, then it offers `Theme: Dark/Light/System` entries with exactly one `checked`, and `Toggle Attention Rail` with shortcut `Cmd+Shift+A`; clicking `view.setTheme:light` sets the theme.
- [ ] Given a failed view command, then `ui.status` holds the error, and a following success clears it (status behaviour kept from the scaffold).
- [ ] `engine.commands` lists the five view commands (shell layer) alongside engine commands, with ids and shortcuts unique across both.

## Test / verification plan
- Tests: rewrite/extend `crates/shell/src/tests.rs` (view shape; `ui.set` all-or-nothing table incl. the failing cases above; read-only fields; dialog open/close; each view command through `engine.execute` and through a menu click; shortcut uniqueness across engine + shell). Keep tests consumer-visible; no per-validator unit-test sprawl beyond the failing-input table.
- Focused: `cargo test -p pistudiodashboard-shell`.
- Manual: `cargo run -p pistudiodashboard-shell --example view` shows the new `ui` object.
- Gate: `cargo xtask ci` passes.

## Notes
- Open question flagged from the spec: `filterDaemons` holds **serverIds** (a daemon that has not yet said `Hello` has none and cannot be filtered); the daemon chips in the view must therefore carry `serverId` (optional) in addition to `entryId`. Task-002's chip list includes it; if a chip has no `serverId` it is not selectable.
- TODO(verify) (app-structure.md): where the persisted theme lives — decided above (UiState authoritative at runtime, written to the config file by sprint-005/task-002).
- TODO(verify) (app-structure.md): param-bound menu entries — resolved with the `<command>:<value>` bound id; the frontend `MenuBar` passes the id unchanged to `invoke_command`.
- The webview never resolves `system` on the Rust side (design-system § Theme switching); do not add an OS-appearance hook.
