# Development

## Prerequisites

- **Rust** stable, 1.95 or newer, with `rustup component add clippy rustfmt` and
  `rustup target add wasm32-unknown-unknown`.
- **Node.js** 22.12 or newer (an even, LTS line), with npm.
- **Linux**: a C compiler, pkg-config, and the development packages Tauri builds against.

  | Distribution | Command |
  |---|---|
  | Debian, Ubuntu | `sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev librsvg2-dev libxdo-dev` |
  | Fedora, RHEL | `sudo dnf install gcc pkgconf-pkg-config webkit2gtk4.1-devel librsvg2-devel libxdo-devel` |
  | Arch | `sudo pacman -S gcc pkgconf webkit2gtk-4.1 librsvg xdotool` |

- **Windows**: the MSVC toolchain with the "Desktop development with C++" workload, and the
  WebView2 runtime (part of Windows 11 and of current Windows 10).
- **macOS**: the Xcode command line tools.
- **Optional**: `cargo install cargo-nextest --locked` (the parallel test runner; without it the
  gate falls back to `cargo test`).

Then, once: `npm ci` in `apps/pistudiodashboard`.

## Commands

| Command | What it does |
|---|---|
| `cargo run -p pistudiodashboard` | Run the desktop app with the built frontend. Add `--release` for anything performance-sensitive |
| `npm run app` (in `apps/pistudiodashboard`) | Run the desktop app on Vite's dev server: frontend changes reload at once |
| `cargo xtask web` | The frontend: type check, lint, tests and build (`npm ci` first when needed) |
| `npm run check` / `npm run lint` / `npm test` (in `apps/pistudiodashboard`) | `tsc --noEmit` / oxlint (type-aware; warnings are errors) / the component tests (vitest) |
| `cargo test -p <crate> --lib --tests` | One crate's tests, skipping examples and doctests |
| `cargo test -p <crate> --test <file>` | One integration-test file |
| `cargo nextest run --workspace` | Every Rust test, in parallel across test binaries |
| `cargo clippy -p <crate> --all-targets -- -D warnings` | Lint what you touched |
| `cargo fmt --all` | Format |
| `cargo xtask layers` | Print the dependency table and any violations |
| `cargo xtask wasm` | `cargo check` for wasm32 on the layered crates |
| `cargo xtask ci` | The whole gate. Run it before you call something done |
| `npm run bundle` (in `apps/pistudiodashboard`) | Build installers with the Tauri bundler |

The gate runs, in order: `fmt`, `web`, `clippy`, `layers`, `test`, `panic_hunt`, `doctest`, `wasm`.

The frontend is embedded into the desktop binary at compile time, from `apps/pistudiodashboard/dist`. Any cargo
command that compiles the app (`cargo run -p pistudiodashboard`, `cargo clippy --workspace`) needs that
directory: run `cargo xtask web` (or `npm run build`) first, and again after frontend changes.

## Run the pieces

The CLI (it has no live connection yet; it lists the command registry):

```sh
cargo run -q -p pistudiodashboard-cli -- commands --filter feed
cargo run -q -p pistudiodashboard-cli -- commands --filter view
cargo run -q -p pistudiodashboard-cli -- config show
cargo run -q -p pistudiodashboard-cli -- config validate path/to/config.json
cargo run -q -p pistudiodashboard-cli -- daemon add --name Dev --host 127.0.0.1 --port 6790
```

The CLI also edits the config file offline (`config path|show|validate`, `daemon list|add|remove`;
`--config FILE` picks another file). It never connects to a daemon and never touches the keychain.

Results are JSON lines on standard output; diagnostics go to standard error. Exit status 1 is a
failed command, 2 a usage error.

The UI without the app. `npm run snapshot` draws the real frontend in a headless browser and saves
a PNG. The view it draws comes from the real shell (seeded with a few agents in several workspaces; `--view FILE` draws a saved view), after the control-protocol calls in `--script`.
`--agents N` adds N bulk agents (a long, virtualised list) and `--scroll-to-slice` scrolls the grid to where
the shell's card slice starts, e.g. `--agents 500 --scroll-to-slice --script '[["ui.set", {"groupBy": "workspace", "gridWindow": {"start": 240, "count": 30}}]]'`.

```sh
cd apps/pistudiodashboard
npm run snapshot:install        # once: downloads the headless browser
npm run build
npm run snapshot -- --out ui.png --size 720x420 \
  --script '[["ui.set", {"theme": "light"}]]'
```

This is how to check a UI change without opening a window. The browser is Playwright's Chromium
(or `--browser webkit`), not the platform webview the app runs in: trust it for layout and themes,
and check platform behaviour in the running app.

The same view as JSON, without any browser (`--empty` starts from an empty monitor):

```sh
cargo run -q -p pistudiodashboard-shell --example view -- --script '[["ui.set", {"theme": "light"}]]'
```

The running app, driven from outside: see `docs/control-protocol.md`.

The app without a window (engine, daemon connections and control server; no display needed):

```sh
cargo run -p pistudiodashboard -- --headless --control 7878 --control-token-file control.token \
  --config-dir /tmp/dashboard-config
```

`PI_STUDIO_LISTEN=127.0.0.1:6790` makes startup detect a daemon on another port. The config
directory defaults to `<platform config dir>/com.avatsaev.pistudiodashboard`
(`PISTUDIODASHBOARD_CONFIG_DIR` or `--config-dir` override it).

## Environment variables

| Variable | Effect |
|---|---|
| `PISTUDIODASHBOARD_LOG` | `trace`, `debug`, `warn` or `error`: the level for Pi Studio Dashboard's own crates (default `info`). Other crates log at `warn` |
| `PISTUDIODASHBOARD_CONTROL_PORT` | Same as `--control <port>` |
| `PISTUDIODASHBOARD_CONTROL_TOKEN_FILE` | Same as `--control-token-file <file>` |
| `CARGO_PROFILE_DEV_DEBUG=true` | Full debug info for one build, to inspect local variables in a debugger |

## Without a display

Running the window needs a display: `Xvfb :99 -screen 0 1280x800x24 &` and then
`DISPLAY=:99 cargo run -p pistudiodashboard`. The snapshot tool and every test need none.

## Debugging the frontend

In a debug build, right-click in the window and choose "Inspect Element" for the webview's
developer tools. Release builds have them off.
