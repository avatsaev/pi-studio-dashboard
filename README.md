# Pi Studio Dashboard

Pi Studio Dashboard is a native desktop app: a Rust engine in which every user action is a command, and a
[Tauri](https://tauri.app) window whose webview (React + TypeScript) draws the state the engine
publishes and sends command ids back. One engine serves every frontend: the desktop window, the
headless CLI, tests and automation.

Its purpose is to monitor pi-studio daemons: it will show every agent of every configured daemon
on one card grid, surface the dialogs that need a human, and let you answer or interrupt. Today the
core exists (wire protocol, live-state model and reducers, config file, daemon client, engine,
shell) and the window draws an empty monitor: the app is not connected to any daemon yet, and
there are no daemon commands or settings in the UI.

## Build and run

Prerequisites are listed in [docs/development.md](docs/development.md).

```sh
(cd apps/pistudiodashboard && npm ci)               # once: the frontend's dependencies
cargo xtask ci                          # the gate: fmt, frontend, clippy, layering, tests, command fuzz, doctests, wasm check
cargo run -p pistudiodashboard                      # the window
cargo run -q -p pistudiodashboard-cli -- commands   # the engine's command registry, as JSON lines
```

While working on the frontend, `npm run app` in `apps/pistudiodashboard` starts the window on Vite's dev server
with hot reload. `npm run bundle` builds installers.

## Where things are

| Path | What |
|---|---|
| `crates/protocol` | L0: the pi-studio wire protocol |
| `crates/model` | L1: `MonitorState`, its bounds, daemon events and effects |
| `crates/ingest` | L2: pure reducers and view derivations |
| `crates/config` | L3: the config file |
| `crates/client` | L4: connections to daemons (not yet used by the engine or the app) |
| `crates/engine` | L5: `Session`, the command registry and the effect executor |
| `crates/shell` | L6: UI state, the generated menu bar and the control protocol, with no UI toolkit in it |
| `crates/testkit` | Test helpers |
| `apps/pistudiodashboard` | The desktop app's web frontend (React + TypeScript): thin, it draws what the shell sends |
| `apps/pistudiodashboard/src-tauri` | The desktop app's Rust binary: the Tauri window and the control server |
| `apps/pistudiodashboard-cli` | The headless CLI |
| `xtask` | `cargo xtask layers \| web \| wasm \| test \| ci` |

Read [AGENTS.md](AGENTS.md) for the rules and [docs/architecture.md](docs/architecture.md) for the
design.

## Licence

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT licence](LICENSE-MIT), at your option. Bundled third-party assets are listed in
[ATTRIBUTION.md](ATTRIBUTION.md).
