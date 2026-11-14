# Architecture

A headless engine in which every action is a command over immutable snapshots of a data model, a
shell that turns the engine's state into one value to draw, a webview that draws it, and all
platform code at the edge.

## Layers

A crate may depend only on crates in lower layers. Nothing below the app binary may use a UI
toolkit, a webview or a windowing crate (tauri, wry, tao, muda, rfd). `cargo xtask layers` checks
both on every run of the gate; the table it checks is `TABLE` in `xtask/src/layers.rs`, and a crate
missing from it fails the gate.

| Layer | Crates | What belongs here |
|---|---|---|
| L0 | `protocol` | The pi-studio wire protocol: frames, handshake, lenient decoding, the outbound request enum. Standalone: no workspace dependency |
| L1 | `model` | `MonitorState` and its bounds, `DaemonEvent`s and `Effect`s: plain serde data and the invariants that keep it valid |
| L2 | `ingest` | Pure reducers that fold `DaemonEvent` batches into `MonitorState`, and the view derivations (`card_order`, `usage_totals`, ...) |
| L3 | `config` | The dashboard's config file: model, validation, atomic load and save |
| L4 | `client` | The tokio runtime: connections to daemons, producing `DaemonEvent` batches. The only layered crate that may use tokio; native only |
| L5 | `engine` | `Session` and the command registry: the façade every frontend talks to. Runs the effects the reducers ask for |
| L6 | `shell` | UI state, the menu bar generated from the registry, shortcuts, the control protocol. No UI toolkit: it produces the view as data |
| testkit | `testkit` | Test helpers; other crates use it as a dev-dependency only |
| apps | `pistudiodashboard` (`apps/pistudiodashboard/src-tauri`), `pistudiodashboard-cli` | Binaries: argument parsing, the Tauri window, sockets, OS integration |
| frontend | `apps/pistudiodashboard/src` | React + TypeScript. Draws the view, sends ids. Not a crate, and not a place for logic |

The layer numbers have gaps on purpose: a crate added later slots in without renumbering. The
engine is not yet wired to `client`: connecting to daemons arrives with the app and engine wiring.

Where new code goes, stopping at the first yes:

1. Plain data the live state consists of: **L1** (`model`).
2. A pure function over that data (a reducer, a derivation): **L2** (`ingest`).
3. Bytes in and out of the app's own config file: **L3** (`config`).
4. Someone else's protocol: wire types in **L0** (`protocol`), the connection that speaks it in
   **L4** (`client`).
5. Something a user does: a **command in L5** that calls the code above.
6. State the UI shows that is not the model, a menu, a shortcut: **L6**, as data.
7. Something drawn or clicked: **the frontend**, as a component that reads the view.
8. Something only the platform can do (a dialog, a socket, a tray icon, a second window): **the
   app binary**, behind a service hook.

## The five seams

1. **`Session::execute(id, params) -> Result<Value>`** (`crates/engine/src/session.rs`) is the only
   way anything happens. It looks the id up, checks the params are a JSON object, checks the
   command's `enabled` predicate, and runs the command inside `catch_unwind`.
2. **The command registry** (`crates/engine/src/commands.rs`): one `CommandSpec` per command (id,
   label, menu, shortcut, params doc, `enabled`, `run`), collected from `<area>_cmds.rs` modules.
   Menus, shortcuts, `pistudiodashboard-cli commands` and the control channel's `engine.commands` are generated
   from it.
3. **Immutable snapshots.** The current state is an `Arc<MonitorState>`. `Session::update(f)`
   clones it, lets the command change the copy, and publishes the copy and bumps the revision only
   when the command returned `Ok`. A failed or panicking command leaves the state untouched;
   readers hold a snapshot without a lock. There is no history: live data is not an edit.
4. **Service hooks** (`EngineServices` in `crates/engine/src/services.rs`, `Services` in
   `crates/shell/src/services.rs`): optional function hooks the app binary fills in (sending to a
   daemon, keychain, config store, notifications). The shell and the engine never call the OS
   themselves. Where a hook is missing, the feature that needs it is disabled, and an effect that
   needs it is reported as skipped.
5. **The view and the control protocol** (`crates/shell/src/state.rs`, `control.rs`): all UI state
   is one serde struct, and `control::view` is the single value the frontend draws: UI state,
   the monitor view (`document`) and menu bar. Four JSON methods expose the running app (`docs/control-protocol.md`),
   and the webview uses the same four.

## The webview boundary

The frontend talks to Rust through two Tauri commands and one event
(`apps/pistudiodashboard/src-tauri/src/commands.rs`, `bridge.rs`):

| | Does |
|---|---|
| `control(method, params)` | A control-protocol call: `engine.execute`, `engine.commands`, `ui.inspect`, `ui.set` |
| `invoke_command(command)` | A menu click or a shortcut: the shell runs the command, binding the entry's fixed params (`view.setTheme:light`) |
| event `view` | The value to draw, sent whenever it changed |

What a person sees is what `ui.inspect` returns. The frontend holds no document state; what it
keeps to itself (the text being typed, which menu is open) never needs to be seen by automation.

## Threads

- One thread owns the `AppShell` and so the `Session`. Commands run there.
- The webview's commands and the control-socket threads only forward: each request crosses a
  channel to the owner thread, which answers them one at a time. A click and an automation request
  never race.
- Slow work belongs on a worker that computes from a snapshot and hands back a result the owner
  thread applies as one command. Live data follows this: the client's batches enter as one
  `feed.ingest` command, whose effects run on the owner thread after the new state is published.

## What stays out of the core

The engine and everything below it contain no tauri, wry or tao types; no sockets, window handles
or file dialogs; no `std::fs` outside `cfg(not(target_arch = "wasm32"))`; and no global mutable
state. The shell crate holds to the same rules.

## Decisions

| Decision | Choice | Why |
|---|---|---|
| Kind of app | Viewer/monitor + client for a service (pi-studio daemon) | D1, D3 |
| Is there a file? | None. The only persisted file is the dashboard config JSON (daemon entries + settings), owned by `crates/config` | D8, D18 |
| Undo | Off. `edit.undo`/`edit.redo` and `Session` history removed | Live data is not an edit; both interventions are irreversible (D14); D2 |
| Where the state lives | Memory only (`MonitorState`, rebuilt on every connect). Config file persisted on settings/daemon change (whole-file atomic write). Secrets in the OS keychain | D8, D18 |
| Slow work and how it reaches the owner thread | All network I/O on the client runtime; results become `DaemonEvent` batches that cross a channel and enter as one `feed.ingest` command on the owner thread. Keychain/config disk I/O runs on a small worker thread and returns as a command result or `feed.ingest` event | D28 |
| Async or network code, which crate owns the runtime | `crates/client` owns the tokio runtime (multi-thread, 2 workers). Nothing else links tokio | Rule 4/7 (wasm check), D3 |
| Live data: ingest command, buffer bound, batch throttle | Command `feed.ingest` (internal, non-recording). Bounds: drill-down ≤ 2,000 items, event ring 500/daemon (D17), log buffer 5,000 lines (stage 2, the daemon-health-and-logs spec). Client batches events (≤ 50 ms window) and the app coalesces `view` emits to ≤ 1 per 100 ms | D17, D28 |
| Windows | One main window (`main`). Tray icon is not a window. `UiState` has no window set | D14 |
| Open models | One `MonitorState`; many daemons inside it | D3 |
| Browser build | Not now. Core crates stay wasm32-checkable (rule 7); `client` and the app are native only | D34 |
| Heavy custom drawing | Not needed: DOM card grid, virtualised above ~100 cards (D28); no canvas | D28 |
| Web content that is not the app's own UI | None. "Open in web UI" opens the daemon's web UI URL in the **system browser**, never in the webview | D24 |
| UI state automation sees vs component-local | See `crates/shell/src/state.rs` (`UiState`); selection/drill-down and panel subscriptions live in `MonitorState` (see rationale there); typed-but-unsubmitted text and open popovers stay in the frontend | rule 5 |
| Platforms | Linux + macOS; Windows deferred; unsigned builds | D34 |
| Attribution of design values | Ported `--pi-*` token values; provenance row in `ATTRIBUTION.md` | D20 |
