# Architecture — App Structure

> Part of: [overview.md](../overview.md)
> Dependencies: [protocol-client.md](protocol-client.md), [config-and-secrets.md](config-and-secrets.md), [live-state.md](live-state.md), [design-system.md](design-system.md), [daemon-connections.md](../features/daemon-connections.md), [agent-monitor.md](../features/agent-monitor.md), [attention-and-interventions.md](../features/attention-and-interventions.md), [notifications-and-tray.md](../features/notifications-and-tray.md), [daemon-health-and-logs.md](../features/daemon-health-and-logs.md)
> Source brief: [daemon-monitoring-dashboard.md](../../grills/daemon-monitoring-dashboard.md) (D2, D8, D14, D17, D18, D22, D28, D32, D34; golden rules 1–9 in `AGENTS.md`)

## Purpose

Fix the crate/layer map, thread model, command registry, `UiState`, `Services` hooks and webview boundary of Pi Studio Dashboard. Kind of app (per `av-rust-desktop-design/references/app-kinds.md`): **viewer/monitor + client for a service**. There is no file format and no undo: live data is not an edit. This spec adapts the existing scaffold (`docs/architecture.md`, `docs/control-protocol.md`, `crates/*`, `apps/pistudiodashboard/src-tauri`) and replaces its sample checklist domain. Other specs own the contents of each crate; this spec owns where things live and how they talk.

## Public contract

### Layer / crate map

Package names carry the prefix `pistudiodashboard-` (`PREFIX` in `xtask/src/layers.rs`). Every crate below MUST be added to `TABLE` in `xtask/src/layers.rs` (a crate missing from it fails the gate). A crate may depend only on lower layers.

```
 frontend  apps/pistudiodashboard/src      React + TS: draws `view`, sends command ids   (no logic)
    |  2 Tauri commands + 1 event (control / invoke_command / `view`)
 app bin   apps/pistudiodashboard/src-tauri   window, tray, notifications, keychain, config dir,
    |                                         open-URL, local-daemon detection, client wiring,
    |                                         view coalescing (<= 1 emit / 100 ms, D28)
    |   owns --- client runtime thread(s) ---> L4 client (tokio)         NOT wasm-checked
    v
 L6  shell    UiState + view + menus + control protocol
 L5  engine   Session + command registry + EngineServices hooks (send_to_daemon, ...)
 L4  client   tokio runtime, WebSocket per daemon, outbound allowlist  (engine does NOT depend on it)
 L3  config   dashboard config file: parse/validate/serialize, atomic write, .bak recovery
 L2  ingest   pure reducers apply(state, DaemonEvent) -> state, derivations, bounded buffers
 L1  model    MonitorState: plain serde data + all caps (D17, D28)
 L0  protocol pi-studio wire types, envelope encode/decode, size caps (no workspace deps)
 testkit      test helpers (dev-dependency only)
 apps/pistudiodashboard-cli   registry automation + offline config commands (exempt from layers)
```

| Layer | Crate (dir / package suffix) | Responsibility | Allowed workspace deps | wasm32 check |
|---|---|---|---|---|
| L0 | `crates/protocol` / `protocol` | Lenient serde wire types for only the messages used (unknown fields tolerated, unknown enum values → `Unknown`, D22), envelope encode/decode, size caps (16 MiB frame, D28). No I/O. Declared `Class::Standalone` | none | yes |
| L1 | `crates/model` / `model` (replaces `doc`) | `MonitorState`: daemons, agents, pending dialog items (`agent_ui_request` with `expectsResponse`), drill-down buffer, event rings, usage, observability buffers; all caps | protocol | yes |
| L2 | `crates/ingest` / `ingest` (replaces `ops`) | Pure `apply(state, DaemonEvent) -> state`; derivations (card order, summaries, "N of M" totals); bounded-buffer push helpers | protocol, model | yes |
| L3 | `crates/config` / `config` (replaces `format`) | Dashboard config file (daemon entries + settings): parse, validate, serialize, atomic write, `.bak` recovery; file I/O behind `cfg(not(target_arch = "wasm32"))` | protocol (only if it shares host/port validators; otherwise none) | yes |
| L4 | `crates/client` / `client` | Owns the tokio runtime; per-daemon connection tasks; outbound allowlist; emits `DaemonEvent` batches | protocol, model, ingest, config (types only) | **no** (excluded; the engine does not depend on it) |
| L5 | `crates/engine` / `engine` | `Session` + command registry; `feed.ingest`; `EngineServices` hooks | protocol, model, ingest, config | yes |
| L6 | `crates/shell` / `shell` | `UiState`, view, generated menus, shortcuts, control protocol, UI-only `Services` | engine (+ below) | yes |
| testkit | `crates/testkit` | Fixtures loader, scripted fake daemon helpers | any (dev-dependency only) | n/a |
| apps | `apps/pistudiodashboard/src-tauri`, `apps/pistudiodashboard-cli` | Binaries; the only place that depends on `client` and on tauri/keyring/notification crates | everything | no |

Layer numbers: L4 `client` sits at layer 4 in `TABLE` (`Class::Layer(4)`) so it may use L0–L3 but the engine (L5) deliberately does not list it. The architecture check (`cargo xtask layers`) MUST additionally assert that `engine`, `shell` and every crate of L0–L3 have **no** dependency on `client` and none on `tokio`, `tokio-tungstenite`, `keyring` or any tauri crate. [TODO(verify): whether `xtask/src/layers.rs` supports a per-crate forbidden-dependency list or needs a small extension; the scaffold only enforces "lower layers only".]

Placement of the dependency direction between L4 and L5 is intentional: the client produces `DaemonEvent` values (defined in `protocol`/`model`) and the app binary carries them to `Session` as `feed.ingest` (see Threads). The engine is therefore testable with scripted `DaemonEvent` sequences and no sockets.

### What replaces the sample domain

| Scaffold | Becomes | Notes |
|---|---|---|
| `crates/doc` (L1, checklist document) | `crates/model` — `MonitorState` | Same shape rules: plain serde, invariants enforced in the model |
| `crates/ops` (L2, history) | `crates/ingest` — reducers/derivations | **History is removed.** No undo stack, no `Session::edit` history step |
| `crates/format` (L3, file format `EXTENSION`) | `crates/config` — config file | No document file exists. `pick_file`/`PickKind` and the open/save/new/close commands are deleted |
| `engine/item_cmds.rs` (`item.*`) | deleted | Replaced by `daemon_cmds.rs`, `agent_cmds.rs`, `settings_cmds.rs`, `feed_cmds.rs` |
| `engine/file_cmds.rs` (`file.*`) | deleted | |
| `engine/edit_cmds.rs` (`edit.undo`, `edit.redo`) | **deleted** | "No undo" decision (record below). They are not "disabled" stubs: a permanently disabled menu item is dead weight, and an interrupt or dialog answer is irreversible by design (D14). `Session` keeps no history |
| `engine/query_cmds.rs` (`document.inspect`) | kept, reworked | `document.inspect` returns the derived monitor view (daemons, cards, rail, drill-down, status counts); name kept so `ui.inspect`'s `document` field and the control protocol stay unchanged |
| `shell` `hide_done`, `ui.set {hideDone}` | replaced | `ui.set` fields become those of `UiState` below |
| `shell` view commands `view.theme.dark/light` | replaced by `view.setTheme` | |
| `shell` file `Services::pick_file` | removed | replaced by the hooks listed below |
| `apps/pistudiodashboard-cli` item/file commands | removed | CLI keeps `commands` and offline config commands only |
| `src/tokens.css` template tokens | replaced | see [design-system.md](design-system.md) |
| `src/components/Items*` | replaced | card grid, rail, drill-down, status bar components |

The mutation invariant of the scaffold ("a failed or panicking command leaves state untouched") is kept via a cloned-then-published `Arc<MonitorState>`; only the history step is removed. [TODO(verify): read `crates/engine/src/session.rs` to decide whether to keep `Session::edit(label, f)` as `Session::update(f)` (snapshot publish without history) or to simplify further; behavior contract is the same either way.]

### Decision record

To be copied into `docs/architecture.md` § Decisions during implementation.

| Decision | Choice | Why |
|---|---|---|
| Kind of app | Viewer/monitor + client for a service (pi-studio daemon) | D1, D3 |
| Is there a file? | None. The only persisted file is the dashboard config JSON (daemon entries + settings), owned by `crates/config` | D8, D18 |
| Undo | Off. `edit.undo`/`edit.redo` and `Session` history removed | Live data is not an edit; both interventions are irreversible (D14); D2 |
| Where the state lives | Memory only (`MonitorState`, rebuilt on every connect). Config file persisted on settings/daemon change (whole-file atomic write). Secrets in the OS keychain | D8, D18 |
| Slow work and how it reaches the owner thread | All network I/O on the client runtime; results become `DaemonEvent` batches that cross a channel and enter as one `feed.ingest` command on the owner thread. Keychain/config disk I/O runs on a small worker thread and returns as a command result or `feed.ingest` event | D28 |
| Async or network code, which crate owns the runtime | `crates/client` owns the tokio runtime (multi-thread, 2 workers). Nothing else links tokio | Rule 4/7 (wasm check), D3 |
| Live data: ingest command, buffer bound, batch throttle | Command `feed.ingest` (internal, non-recording). Bounds: drill-down ≤ 2,000 items, event ring 500/daemon (D17), log buffer 5,000 lines (stage 2, [daemon-health-and-logs.md](../features/daemon-health-and-logs.md)). Client batches events (≤ 50 ms window) and the app coalesces `view` emits to ≤ 1 per 100 ms | D17, D28 |
| Windows | One main window (`main`). Tray icon is not a window. `UiState` has no window set | D14 |
| Open models | One `MonitorState`; many daemons inside it | D3 |
| Browser build | Not now. Core crates stay wasm32-checkable (rule 7); `client` and the app are native only | D34 |
| Heavy custom drawing | Not needed: DOM card grid, virtualised above ~100 cards (D28); no canvas | D28 |
| Web content that is not the app's own UI | None. "Open in web UI" opens the daemon's web UI URL in the **system browser**, never in the webview | D24 |
| UI state automation sees vs component-local | See `UiState` below; selection/drill-down and panel subscriptions live in `MonitorState` (see rationale there); typed-but-unsubmitted text and open popovers stay in the frontend | rule 5 |
| Platforms | Linux + macOS; Windows deferred; unsigned builds | D34 |
| Attribution of design values | Ported `--pi-*` token values; provenance row in `ATTRIBUTION.md` | D20 |

### Command registry

Ids are camelCase `<area>.<verb>` (as in the shared contract). All commands are dispatched through `Session::execute(id, params)` (or the shell's `ShellCommand` mechanism for pure `UiState` changes); every one is listed by `engine.commands` and the CLI `commands`.

"Mutates daemon?" = the command causes a **mutation** request on a daemon (anything other than a read RPC). Exactly two may (D2 as revised by D36, D37): `dialog.respond` → `agent_ui_respond_request` and `agent.interrupt` → `interrupt_agent`. The tool-permission family was retired in pi-studio (sprint-086) and the dashboard has no permission command or decoding (D37). Subscribe/unsubscribe, list, stats and timeline requests are read RPCs and not mutations.

| id | Label | Menu | Shortcut | Params | Enabled when | Layer | Mutates daemon? |
|---|---|---|---|---|---|---|---|
| `daemon.add` | Add Daemon… | Daemon | Cmd+N | `{name, host, port, tls, webUiUrl?, password?}` (UI opens the form dialog when invoked from a menu/shortcut; the form submit calls this with params) | always | engine | n |
| `daemon.edit` | Edit Daemon… | Daemon | – | `{entryId, name?, host?, port?, tls?, webUiUrl?}` | an entry exists | engine | n |
| `daemon.remove` | Remove Daemon | Daemon | – | `{entryId}` (deletes the keychain password too, D18; clears that daemon's cards) | an entry exists | engine | n |
| `daemon.connect` | Connect | Daemon | – | `{entryId}` | entry exists and is not connecting/connected | engine | n |
| `daemon.disconnect` | Disconnect | Daemon | – | `{entryId}` | entry is connecting/connected/reconnecting | engine | n |
| `daemon.setPassword` | Set Password… | Daemon | – | `{entryId, password}` (stores in keychain, or holds it in memory for the session when no keychain; D18, D29) | an entry exists | engine | n |
| `daemon.showDiagnostics` | Daemon Diagnostics | Daemon | Cmd+I | `{entryId}` (opens the per-daemon diagnostics panel, D29) | an entry exists | shell | n |
| `daemon.detectLocal` | Detect Local Daemon | Daemon | – | `{}` — the shell runs the `detect_local_daemon` hook, then executes `daemon.add` with the result and `autoDetected: true` (also the empty-state button) | `detect_local_daemon` hook present and no auto-detected entry exists | shell | n |
| `agent.select` | – (card click; no menu) | – | – | `{serverId, agentId}` (opens drill-down, issues `fetch_agent_timeline_request`) | agent exists and is not archived | engine | n |
| `agent.deselect` | Close Drill-down | View | Esc (when drill-down open) | `{}` | a drill-down is open | engine | n |
| `agent.interrupt` | Interrupt Agent… | Agent | – | `{serverId, agentId, confirm: true}` — `confirm` MUST be `true` or the command fails with "confirmation required" (D14; the frontend shows the confirm dialog first) | agent status is `running` and its daemon is connected and the request is not already in flight (D27) | engine | **y** (`interrupt_agent`) |
| `agent.openInWebUi` | Open in Web UI | Agent | – | `{serverId, agentId}` → `open_url(<webUiUrl>#agent=<urlencoded agentId>)` (D24) | daemon entry has a web UI URL | engine | n |
| `dialog.respond` | – (rail/drill-down button; no menu) | – | – | `{serverId, requestId, response}` — `response` is the answer for the dialog's `method`: `confirm` → yes/no, `select` → one of the offered options; `input`/`editor` are never answerable (hand-off only) (D36) | dialog pending, `expectsResponse` true, method `confirm` or `select`, not in flight, daemon connected | engine | **y** (`agent_ui_respond_request`) |
| `view.setGridWindow` | – (frontend scroll; no menu) | – | – | `{start, count}` (virtualised card-grid window, D28; clamped to the card count) | always | shell | n |
| `window.focusItem` | – (notification click; no menu) | – | – | `{serverId, itemId}` — shows/raises the window (`tray.show_window`) and sets `UiState.focusItem`; unknown/resolved item → toast "already resolved" | always | shell | n |
| `view.filterDaemons` | – (chips) | – | – | `{serverIds: string[]}` (empty = all) | always | shell | n |
| `view.filterStatuses` | – (chips) | – | – | `{statuses: AgentStatus[]}` (empty = all) | always | shell | n |
| `view.toggleAttentionRail` | Toggle Attention Rail | View | Cmd+Shift+A | `{}` | always (while the drill-down is open the rail is hidden; the toggle still flips the saved collapsed state, D15) | shell | n |
| `view.setTheme` | Theme: Dark / Light / System | View | – | `{theme: "dark"\|"light"\|"system"}` | always | shell (persisted via `settings`) | n |
| `settings.setNotification` | Notify on Needs-input / Failures | View | – | `{kind: "needsInput"\|"failures", enabled: bool}` | always | engine | n |
| `settings.setCloseToTray` | Close to Tray | View | – | `{enabled: bool}` | tray hook available | engine | n |
| `feed.ingest` | – (internal, no menu) | – | – | `{events: DaemonEvent[]}` (batched, non-recording) | always; rejected (error) if called with a malformed batch, leaving state untouched | engine | n |
| `feed.tick` | – (internal, no menu) | – | – | `{now}` — runs `expire_tick` (dialog expiry, ≤ 1 Hz while any dialog has `expiresAt`) and `stats_tick` (every 10 s while a connected agent is running); called by the app binary's owner loop (D16, D39) | always | engine | n |
| `health.open` *(stage 2)* | Daemon Health | Daemon | Cmd+H | `{serverId}` | daemon connected and advertises `daemonObservability` | engine | n |
| `health.close` *(stage 2)* | Close Health | Daemon | – | `{}` | a health panel is open | engine | n |
| `logs.open` *(stage 2)* | Daemon Logs | Daemon | Cmd+L | `{serverId, minLevel?}` | as `health.open` | engine | n |
| `logs.close` *(stage 2)* | Close Logs | Daemon | – | `{}` | a logs panel is open | engine | n |
| `logs.setMinLevel` *(stage 2)* | Log Level | Daemon | – | `{minLevel: "trace"\|"debug"\|"info"\|"warn"\|"error"\|"fatal"}` | a logs panel is open | engine | n |

Rules:
- `view.*` commands that only change `UiState` are shell commands (no engine, no recording), as in the scaffold. `view.setTheme` additionally asks the engine to persist the choice: shell runs it, then executes `settings` persistence through the config hook (see Services). [TODO(verify): the exact split — whether the persisted `theme` lives in `MonitorState.settings` (engine-owned, shell mirrors) or in `UiState.theme` with the shell calling `config_store.save`. Contract either way: one source of truth, persisted in the config file, `ui.inspect` shows it.]
- Menu entries that need a typed choice (`view.setTheme`, `logs.setMinLevel`) appear as one menu entry per value bound to fixed params, each with a `checked` state. [TODO(verify): the scaffold's `ShellCommand` has fixed ids and `run: fn(&mut AppShell)`; confirm that param-bound entries of one command id fit `menus.rs`, otherwise keep one thin `ShellCommand` per value that delegates to the one command.]
- Menu order (`MENU_ORDER`): `Daemon`, `Agent`, `View`. `File`, `Edit` and `Item` are removed; quit uses the platform application menu. [TODO(verify): macOS application-menu items (About, Quit) handling in `src-tauri` `main.rs`.]
- Commands with a missing/unknown daemon or agent key fail with a message ("unknown daemon", "agent is gone"), never panic (rule 2).
- **Daemon-mutation guard (D2, revised by D36, D37).** The mutation set is closed at three levels: (1) only `dialog.respond` and `agent.interrupt` build a mutating `OutboundRequest`; (2) `OutboundRequest` is an enum whose variants are the read RPCs plus exactly those two; (3) the client's allowlist rejects any other envelope type. Tests: a registry test that sweeps every command id with representative params through a recording fake `send_to_daemon` and asserts the only mutation request types are `agent_ui_respond_request` and `interrupt_agent`; a client test that the encoder cannot produce any other mutation type. See [protocol-client.md](protocol-client.md).
- No command carries a password into `MonitorState`, `UiState`, any view value, or any log line. `daemon.add`/`daemon.setPassword` params are redacted from control-socket logging (D29, D32).

### UiState

`UiState` (`crates/shell/src/state.rs`, serde, `camelCase`, `#[serde(default)]`) is what `ui.inspect` returns under `ui` and `ui.set` changes. It replaces `hideDone`.

| Field | Type | Meaning | `ui.set`? |
|---|---|---|---|
| `theme` | `"dark" \| "light" \| "system"` (extends `ThemeKind` with `System`; default `dark`, D21) | The user's *choice*; the webview resolves `system` (see [design-system.md](design-system.md)) | yes |
| `filterDaemons` | `string[]` serverIds, empty = all | Daemon chips | yes |
| `filterStatuses` | `AgentStatus[]`, empty = all | Status chips | yes |
| `attentionRailCollapsed` | bool | D15 | yes |
| `focusItem` | `null \| {serverId, itemId}` | Attention item the window was asked to focus (notification click, `window.focusItem`); the frontend scrolls/highlights it and the rail expands | yes (clear only) |
| `gridWindow` | `{start, count}` | Virtualised card-grid window (D28), set by `view.setGridWindow` | yes |
| `dialog` | `null \| {kind: "daemonForm", mode: "add"\|"edit", entryId?} \| {kind: "password", entryId} \| {kind: "confirmInterrupt", serverId, agentId} \| {kind: "removeDaemon", entryId}` | Which modal is open | yes (open/close only; submit goes through commands) |
| `diagnosticsTarget` | `null \| entryId` | Open diagnostics panel (D29) | yes |
| `toasts` | `{id, kind, text}[]` (bounded, 5) | Transient notices ("answered elsewhere", "interrupt: agent not running"), expire by frontend timer; produced by commands/ingest results | read-only |
| `status` | string | Last command's error for the status bar (kept from scaffold) | read-only |

Not in `UiState`, with rationale:
- **Selected agent / drill-down target.** Ingest must route `agent_stream` events into the drill-down buffer, and must close it on archive/delete/`agent_timeline_reset` (D27); a pure reducer can only do that if the target lives in `MonitorState` (`drilldown.target`). `agent.select`/`agent.deselect` are therefore engine commands and the view carries the target under `document`. (Deviation from "UiState: selection": single source of truth wins over a mirrored field.)
- **Health/logs panel open state, `minLevel`, subscription flags (stage 2).** Same reason: they decide which daemon pushes are kept and what unsubscribes on disconnect. They live in `MonitorState.observability` and are shown under `document`.
- **Component-local (never visible to automation):** text being typed in forms and the password field, popovers, scroll positions, hover/focus, per-card "expanded tool call" toggles, virtual-list windowing, toast timers.

`ui.set` stays all-or-nothing validated (unknown field or bad value → error, nothing changes). `docs/control-protocol.md` and `apps/pistudiodashboard/src/lib/types.ts` are updated in the same change as `UiState`.

### Services hooks

The scaffold's `Services` (shell crate) is a struct of optional hooks; a missing hook disables the feature that needs it. Hooks split by who calls them:

**`EngineServices`** (defined in `crates/engine`, held by `Session`, called on the owner thread only; hooks are non-blocking and capture channel senders):

| Hook | Signature (behavioral) | Used by |
|---|---|---|
| `send_to_daemon` | `(serverId, OutboundRequest) -> Result<(), SendError>` — enqueue on the client runtime; never blocks; `SendError` = not connected / queue full / not allowed | `dialog.respond`, `agent.interrupt`, `agent.select` (timeline fetch), `health.*`, `logs.*`, stats polling triggers |
| `daemon_link` | `(entryId, LinkCommand)` with `Connect \| Disconnect \| Reconfigure(entry) \| Remove` — controls the connection task for an entry | `daemon.add/edit/remove/connect/disconnect` |
| `keychain` | `get(entryId)`, `set(entryId, password)`, `delete(entryId)` → `Result<_, KeychainError>` (`Unavailable`, `Denied`, `NotFound`); implemented with `keyring`; runs on the I/O worker (may block on OS prompts) | `daemon.add/remove/setPassword`, connect-time password lookup (D18, D29) |
| `config_store` | `load() -> ConfigLoad`, `save(Config) -> Result` (atomic write + `.bak`, implemented by `crates/config`) | `daemon.*`, `settings.*`, `view.setTheme` |
| `open_url` | `(url) -> Result` — system browser only; scheme restricted to `http`/`https` | `agent.openInWebUi` |
| `notify` | `(Notification{title, body, target: (serverId, requestId/agentId)}) -> ()` — OS notification; click routes back as a shell focus request | notifications effect ([notifications-and-tray.md](../features/notifications-and-tray.md)) |

**`Services`** (shell crate, UI-only; remains in `crates/shell/src/services.rs`, `pick_file` removed):

| Hook | Behavior | Used by |
|---|---|---|
| `tray` | `set_badge(count)`, `set_tooltip(text)`, `show_window()`; absent on platforms without a tray → `settings.setCloseToTray` disabled | notifications/tray |
| `config_dir` | path of the app config dir (config file, `.bak`, the app's own log file) | startup |
| `detect_local_daemon` | reads `$PI_STUDIO_HOME/config.json` listen address → `host:port`, falling back to `127.0.0.1:6767` (D10, D29) | startup auto-add (via `daemon.add` with `autoDetected`) |

All platform code (tauri, keyring, notification plugin, tray) is implemented in `apps/pistudiodashboard/src-tauri/src/services.rs` (extended) and wired in `main.rs`. [TODO(verify): whether `notify` fits better as a shell hook than an engine hook given that the decision to notify is made from ingest results observed by the owner thread; behavior contract (called on owner thread, non-blocking) is the same.]

### Webview boundary (unchanged)

| | Does |
|---|---|
| `control(method, params)` | `engine.execute`, `engine.commands`, `ui.inspect`, `ui.set` — unchanged methods and semantics |
| `invoke_command(command)` | Menu click or shortcut. Commands that need typed input open a `UiState.dialog` instead of collecting it natively; no native dialog remains |
| event `view` | The value to draw, coalesced to ≤ 1 per 100 ms (D28) |

- `capabilities/default.json` stays `core:default` only: **no plugin permissions for the main window.** Tray (`tray-icon` feature) and notification plugin are driven from Rust only, never granted to the webview.
- CSP unchanged in spirit (`default-src 'self'; … connect-src ipc: http://ipc.localhost`): the webview opens **no** network connection (no `ws:`/`https:` in `connect-src`); all daemon traffic is in `crates/client` (rule 1, D32). No remote content, no webview navigation to daemon web UIs.
- The webview never receives passwords, tokens, `Authorization`/bearer values or raw frames. Typed passwords flow webview → `daemon.add`/`daemon.setPassword` params → keychain; the view never echoes them (D32).
- The control socket (`--control`) keeps token auth and 1 MiB lines; `engine.execute` of any command works over it, so the daemon-mutation guard above applies to automation too. [TODO(verify): whether the control socket should be disabled in release builds or require explicit opt-in; the scaffold only enables it with `--control`.]

## Behavior & algorithms

### Threads

```
owner thread        owns AppShell -> Session -> Arc<MonitorState>; runs every command; answers
                    webview/control requests one at a time (unchanged scaffold behavior)
client runtime      tokio multi-thread (2 workers), owned by crates/client's ClientHandle;
                    one task per daemon entry; no shared mutable state with the owner thread
feed pump thread    (app binary) receives batches from the client's event channel, forwards each as
                    one owner request: execute("feed.ingest", {events}); blocks on nothing else
io worker thread    (app binary) keychain and config-file I/O; results return as command results
                    (or a feed.ingest event for connect-time password lookups)
main/UI thread      Tauri event loop, tray, notifications; never runs commands, never blocks on I/O
control sockets     forward only (unchanged)
```

Inbound (daemon → state):

```
client task:  frame -> decode (size cap, lenient) -> DaemonEvent
              accumulate into a batch for up to 50 ms or 256 events, whichever first
              send batch on bounded channel (capacity 256 batches)
feed pump:    for batch in channel: owner.request(execute("feed.ingest", {events: batch}))
owner:        feed.ingest -> for each event: state = ingest::apply(state, event)   (one published snapshot per batch)
              then: derive Effects (below) from the ingest result
app binary:   after each request, if the view changed and >= 100 ms since last emit -> emit `view`
              (a trailing emit is scheduled so the last change is never lost)
```

Ingest returns **effects** alongside the new state; the owner thread executes them after publishing, so reducers stay pure:

| Effect | Executed as |
|---|---|
| `Notify {kind, serverId, itemId}` | `EngineServices.notify` (needsInput/failures toggles and >3-in-5 s coalescing applied first, D9, D27) |
| `ResolveNotification {serverId, itemId}` | mark/withdraw an already shown notification as resolved |
| `Toast {kind, text}` | append to `UiState.toasts` ("answered elsewhere: …", D27) |
| `CloseDrill {reason}` | clear `drilldown.target`, toast the reason (archived/deleted, D27) |
| `FetchStats {serverId, agentId}` | `send_to_daemon(SessionStats)` read request; triggers: `Snapshot`, turn end, new agent, and the owner loop's 10 s `stats_tick` for running agents — the engine is the only scheduler, the client dedupes (D16, D39) |
| `FetchTimeline {serverId, agentId}` | `send_to_daemon(FetchTimeline)` — one logical (re)load on select, `agent_timeline_reset` or reconnect; the client pages `before` then `after` while `hasNewer` (D17, D27, D39) |

- When the channel is full the client coalesces: it drops only superseded `agent_stream` text/tool deltas for non-selected agents (their summaries are derived values) and **never** drops state-bearing events (agent status, `agent_ui_request`/`agent_ui_resolved`, connection state). It counts drops into diagnostics. [TODO(verify): the exact coalescing rule is specified in [live-state.md](live-state.md) / [protocol-client.md](protocol-client.md); align names.]
- `feed.ingest` is non-recording (no history step exists anyway), has no menu entry, and is exposed by `engine.commands` flagged internal.

Outbound (state → daemon):

```
command (owner thread) -> EngineServices.send_to_daemon(serverId, OutboundRequest)
   -> client's per-daemon command channel (bounded, non-blocking try_send)
   -> connection task checks the allowlist, encodes `{type:"session", message:{type, requestId}}`, sends
   -> response/push returns as DaemonEvent via the inbound path above
```

`dialog.respond`/`agent.interrupt` mark the item "in flight" in `MonitorState` (disabling the button, D27) before sending; failure to enqueue clears the flag and sets `UiState.status`. A `{ok:false, error:"not_found"}` response to `agent_ui_respond_request` takes the "answered elsewhere / no longer pending" path (fixture `agent_ui_respond_response`). **Interrupt does not clear pending dialogs** (pi-studio leaves `agent_ui` dialogs pending after `interrupt_agent`, D36): they stay in the rail until the daemon resolves them or the agent is archived/deleted/closed.

### Startup

```
load config (config_store.load): corrupt -> .bak + empty + warning toast (D29)
if no entry has autoDetected: local = detect_local_daemon(); daemon.add(local, autoDetected=true)
for each entry: daemon_link(Connect) -- password via keychain (missing/denied -> UiState.dialog password prompt)
window opens on `main`; first `view` event after the first ingest or after 100 ms
```

### Wasm check scope

`cargo xtask wasm` (`cmd_wasm`, `xtask/src/main.rs`) checks `protocol`, `model`, `ingest`, `config`, `engine`, `shell` for `wasm32-unknown-unknown`. `client` and both apps are excluded (tokio networking, tungstenite, keyring, tauri). `std::fs` in `config` is behind `cfg(not(target_arch = "wasm32"))`. [TODO(verify): `cmd_wasm` currently derives its list from layers; confirm how to exclude `client` (explicit list vs a `Class` flag).]

### CLI scope

`apps/pistudiodashboard-cli` keeps `commands` (registry listing) and offline config commands (list/add/remove daemon entries and show settings by operating on the config file, never starting a connection). Live monitoring from the CLI is deferred. The CLI uses a `Session` with no `send_to_daemon` hook, so `daemon.connect`, `agent.*`, `dialog.respond` are listed but report `enabled: false` with reason "no live connection in the CLI".

### Platforms

Linux and macOS (D34). Tray/notification behavior differs per platform; macOS needs a first manual run for notification permission. Windows deferred. Builds unsigned. Per the template, only Linux is automatically verified.

## Data & persistence touchpoints

- Persisted: config file in the app config dir (owned by `crates/config`, schema in [config-and-secrets.md](config-and-secrets.md)); nothing else. No agent data, no logs, no dialog payloads on disk (D8, D29).
- In memory: `MonitorState` (see [live-state.md](live-state.md)).
- Secrets: OS keychain only (D18).
- The dashboard's own log file lives in the config dir (`src-tauri/src/logging.rs`, scaffold) and never contains passwords or dialog response values (D29); daemon log lines (stage 2) are never written there.

## Error handling & edge cases

| Condition | Expected behavior |
|---|---|
| Command panics | Contained by `Session::execute` `catch_unwind`; state untouched (scaffold behavior) |
| `feed.ingest` batch with unknown event kinds | Counted and ignored, never fatal (D26) |
| Event for unknown daemon/agent | Dropped and counted; never creates phantom state |
| `send_to_daemon` hook missing | Commands needing it are disabled with reason (CLI, tests) |
| Keychain unavailable | Prompt dialog; nothing persisted (D29) |
| Feed channel backpressure | Client-side coalescing as above; the owner thread is never blocked by the client |
| Command fired for a stale target (agent archived meanwhile) | Error message in `status`, no daemon request sent (D27) |
| Interrupt without `confirm: true` over the control socket | Fails; guard is in the engine, not only the UI (D14) |
| Window closed with close-to-tray on | Window hides; runtime and connections continue (D14) |

## Dependencies on other specs

- [protocol-client.md](protocol-client.md) — `OutboundRequest`, `DaemonEvent`, allowlist, client channels.
- [config-and-secrets.md](config-and-secrets.md) — `config_store` and `keychain` contracts, entry shape.
- [live-state.md](live-state.md) — `MonitorState`, reducers, caps.
- [agent-monitor.md](../features/agent-monitor.md), [attention-and-interventions.md](../features/attention-and-interventions.md), [notifications-and-tray.md](../features/notifications-and-tray.md), [daemon-connections.md](../features/daemon-connections.md) — command semantics and view contents.
- [design-system.md](design-system.md) — tokens/themes consumed by the frontend.
- [daemon-health-and-logs.md](../features/daemon-health-and-logs.md) — stage 2 commands.

## Acceptance criteria

- [ ] Given the workspace, when `cargo xtask layers` runs, then every crate of the map is in `TABLE`, `engine`/`shell`/L0–L3 have no dependency on `client`, `tokio`, `keyring` or tauri crates, and the gate passes. (D34; rules 4, 7)
- [ ] Given `cargo xtask wasm`, then `protocol`, `model`, `ingest`, `config`, `engine`, `shell` check for `wasm32-unknown-unknown` and `client` is excluded. (rule 7)
- [ ] Given the command registry, when `engine.commands` is called, then it lists every id of the table (stage 1 ids always; stage 2 ids once implemented), no `item.*`, `file.*`, `edit.*` id remains, and `feed.ingest` is flagged internal. (rule 3)
- [ ] Given a recording fake `send_to_daemon`, when every command is executed with representative params and every `OutboundRequest` variant is encoded, then the only mutation request types observed are `agent_ui_respond_request` and `interrupt_agent`. (D2, D36, D37)
- [ ] Given a pending `confirm` dialog and an `input` dialog, then only the `confirm` one enables `dialog.respond`; after `agent.interrupt` on that agent both dialogs remain pending. (D36)
- [ ] Given a notification click, when `window.focusItem` runs, then the window is shown and `UiState.focusItem` equals that item; for a resolved item a toast says so. (D14, D27)
- [ ] Given `agent.interrupt` called without `confirm: true`, then it fails and sends nothing. (D14)
- [ ] Given a `feed.ingest` batch of N events, when applied, then exactly one new state snapshot is published and a malformed batch leaves state unchanged. (D28)
- [ ] Given 1,000 `feed.ingest` batches within one second, then the app emits at most 10 `view` events per second and the last state is always emitted. (D28, D33)
- [ ] Given the Tauri config, then `capabilities/default.json` grants only `core:default`, CSP `connect-src` contains no `ws:`/`wss:`/`http(s)` origin other than `ipc:`/`ipc.localhost`, and no view value or log line contains a password. (rule 6; D18, D32)
- [ ] Given `ui.set` with an unknown field or invalid `theme`, then it errors and `UiState` is unchanged. (rule 5)
- [ ] Given the CLI, when `pistudiodashboard-cli commands` runs, then it lists the registry and never opens a network connection. (CLI scope)

## TODO(verify)

- Dependency licences (golden rule 9) before adding each: `keyring` (and its Linux secret-service backend crates), `tauri-plugin-notification` (and Tauri `tray-icon` feature), the WebSocket crate (`tokio-tungstenite` or alternative), `rustls`/`tokio-rustls` and webpki roots, `tokio`. Record results in `ATTRIBUTION.md`/`deny.toml` equivalents. (brief open item "Licences", D34)
- Whether `xtask/src/layers.rs` supports forbidden-dependency assertions (see map).
- `Session::edit` fate once history is removed (`crates/engine/src/session.rs`).
- Param-bound menu entries vs per-value `ShellCommand` wrappers in `crates/shell/src/menus.rs`.
- Where persisted `theme` and `settings.*` live (engine `MonitorState.settings` vs shell), single source required.
- macOS application menu / Quit handling and first-run notification permission.
- Control socket policy in release builds.
- Linux keychain availability in headless/CI (tests must use a fake keychain hook).
- Brief open: "Today's workflow and pain / measured by" remains a non-blocking owner question (avatsaev); does not affect this spec.
