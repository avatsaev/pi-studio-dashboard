# Pi Studio Dashboard — Overview

> Source of truth for what this effort is and how it fits together. Sub-specs describe the details;
> this file ties them together and indexes them.
> Source brief: [../grills/daemon-monitoring-dashboard.md](../grills/daemon-monitoring-dashboard.md)
> (`Status: grilled`, decisions `D1`–`D37`). Every requirement below cites a brief decision. `D36`
> re-pointed answering to Pi's `agent_ui_*` dialogs; `D37` aligned this scope with what pi-studio
> shipped on 2026-11-13 (commit `47d80e6`): its side of stages 1 and 2 is done, and its
> tool-permission family is retired.

## Purpose & scope

Pi Studio Dashboard is a native desktop app (Linux and macOS, D34) that monitors one or more
pi-studio daemons in real time over direct WebSocket connections (D3). In one window it shows:

- every agent on every daemon as a card: status, current activity, usage (D1, D6, D11);
- an attention rail of what needs the engineer: Pi dialogs waiting for an answer
  (`agent_ui_request` with `expectsResponse`), errors and failed turns (D1, D15, D36);
- daemon health, connected clients, version and a live log tail (stage 2, D1, D4, D30).

It can act on a daemon in exactly two ways (D2 as revised by D36, D37): answer a `confirm` or
`select` dialog (`agent_ui_respond_request`), and interrupt an agent after confirmation
(`interrupt_agent`, D14). `input`/`editor` dialogs hand off to the pi-studio web UI (D24, D36). OS
notifications and a tray icon keep it useful in the background (D9, D14).

**Not in scope** (brief *Non-goals* and *Deferred*): full control of agents, transcript/history
browsing, answering `input`/`editor` dialogs, persisted history or charts, terminals, schedules,
loops, chat rooms, git/worktrees, the E2EE relay, Windows, code signing, a dedicated `"monitor"`
client type, generated protocol types, tool-permission requests (retired in pi-studio, D37).

### Delivery stages (D5, D36, D37)

| Stage | Dashboard | pi-studio side |
|---|---|---|
| 1 | Connections, agent monitor, attention + interventions, notifications + tray, design system. Works against every daemon in the field (D26) | **Shipped**: web UI agent deep link (sprint-082), external-client fixtures (sprint-083) |
| 2 | Daemon health and logs panels | **Shipped**: daemon status RPC and log streaming behind `daemonObservability` (sprints 084–085) |

pi-studio's external-client fixtures (`packages/protocol/fixtures/external-clients/`, manifest
`formatVersion` 1) are the wire contract for this app: the dashboard vendors them and deserializes
every frame in its tests (D22, D37).

## Tech stack & runtime requirements

| Concern | Choice |
|---------|--------|
| Language | Rust (core, client, app binary); TypeScript + React only for the thin webview frontend (golden rule 1) |
| Run/business logic layers | Layered crates L0–L6 checked by `cargo xtask layers` — see [architecture/app-structure.md](architecture/app-structure.md) |
| Framework / build | Tauri 2 window; Vite frontend; `cargo xtask ci` is the single gate |
| Async / network | tokio confined to the L4 `client` crate; engine and below are synchronous and build for wasm32 (golden rule 7) |
| Persistence | One dashboard config JSON file (daemon entries + settings) and the OS keychain for passwords (D18). No agent data persisted (D8) |
| Key external integrations | pi-studio daemon WebSocket API + `GET /api/health`; pi-studio web UI (deep link, opened in the system browser); OS notifications, tray, keychain |

## High-level architecture

```
                    ┌────────────────────────── pi-studio daemons (N) ─────────────────────────┐
                    │  ws(s)://host:6767   hello → server_info → session envelopes / pushes    │
                    │  GET /api/health                                                         │
                    └───────────▲──────────────────────────────┬───────────────────────────────┘
                                │ allowlisted requests          │ pushes + responses
                                │ (read RPCs, 2 mutations — D2) │
┌───────────────────────────────┴───────────────────────────────▼──────────────────────────────┐
│ L4 client (tokio): one task per daemon — probe, connect, handshake, sync, backoff, polling   │
│   uses L0 protocol (lenient wire types, caps)                                                │
└───────────────▲───────────────────────────────────────────────┬──────────────────────────────┘
                │ Services::send_to_daemon(serverId, req)        │ DaemonEvent batches (channel)
┌───────────────┴───────────────────────────────────────────────▼──────────────────────────────┐
│ app binary (Tauri): owner thread · tray · notifications · keychain · config dir · open URL   │
│   └─ owner thread runs feed.ingest(batch) and user commands on the Session                   │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ L6 shell: UiState + view + control protocol      L5 engine: Session + command registry        │
│ L3 config: config file    L2 ingest: pure reducers/derivations    L1 model: MonitorState      │
└───────────────────────────────────────┬───────────────────────────────────────────────────────┘
                                        │ event "view" (≤ 1 per 100 ms, D28)   ▲ control(...) / invoke_command(...)
                                ┌───────▼──────────────────────────────────────┴────────┐
                                │ webview (React): card grid · attention rail ·          │
                                │ drill-down · dialogs · status bar — draws the view     │
                                └────────────────────────────────────────────────────────┘
```

## Module / directory map

| Path | Responsibility |
|------|----------------|
| `crates/protocol` (L0) | pi-studio wire types, envelope encode/decode, size caps; no I/O |
| `crates/model` (L1) | `MonitorState`: daemons, agents, UI requests, usage, drill-down buffer, event rings — all bounded |
| `crates/ingest` (L2) | Pure reducers `apply(state, DaemonEvent)`, derivations (card order, activity summary, totals) |
| `crates/config` (L3) | Dashboard config file: schema, validation, atomic write, `.bak` recovery |
| `crates/client` (L4) | Per-daemon connection tasks, outbound allowlist, polling; emits `DaemonEvent` batches |
| `crates/engine` (L5) | `Session`, command registry (`daemon.*`, `agent.*`, `dialog.respond`, `view.*`, `settings.*`, `window.focusItem`, `feed.ingest`, stage 2 `health.*`, `logs.*`) |
| `crates/shell` (L6) | `UiState`, view, menus, control protocol |
| `crates/testkit` | Test helpers, fixture loaders |
| `apps/pistudiodashboard/src-tauri` | Window, owner thread, client runtime wiring, tray, notifications, keychain, view coalescing |
| `apps/pistudiodashboard/src` | Thin React frontend; `tokens.css` with pi-studio `--pi-*` tokens |
| `apps/pistudiodashboard-cli` | Registry automation; live monitoring from the CLI is deferred |
| `xtask/` | Gate: fmt, web, clippy, layers, test, panic_hunt, doctest, wasm |

## Data model overview

- **Daemon entry** (persisted): `entryId`, name, host, port, tls, webUiUrl?, autoDetected,
  lastSeenServerId? — password in the keychain only (D18). See
  [architecture/config-and-secrets.md](architecture/config-and-secrets.md).
- **Daemon** (live): connection state, `server_info` (serverId, hostname, real version, features),
  capability set / degraded flags, diagnostics counters (D25, D26, D29).
- **Agent** (live, keyed `serverId`+`agentId`): status, title, model, activity summary, usage
  snapshot or none, attention flags (D6, D16).
- **UI request** (live, keyed `serverId`+`requestId`): `agent_ui_request` with method, title,
  `expectsResponse`, payload summary; `confirm`/`select` answerable, `input`/`editor` hand-off,
  non-dialog surfaces are not attention items (D36). Late-join via `agent_ui_list_request`.
- **Drill-down buffer** for one selected agent (≤ 2,000 items) and **event ring** (500/daemon)
  (D17).
- Stage 2: **daemon status** and **log lines** (bounded) per open panel (D30).

Full shapes: [architecture/live-state.md](architecture/live-state.md).

## External integrations & configuration

| Name | Kind | Use |
|---|---|---|
| pi-studio WebSocket API (`hello`, `status/server_info`, `session` envelopes) | external API | Everything live — [architecture/protocol-client.md](architecture/protocol-client.md) |
| `Sec-WebSocket-Protocol: pi-studio.bearer.<password>` | auth | Optional daemon password (D25) |
| `GET /api/health` | external API | Up/down probe (D5, D25) |
| `$PI_STUDIO_HOME` (default `~/.pi-studio`), its `config.json` `daemon.listen` (default `127.0.0.1:6767`) | local file | Auto-detect the local daemon (D10, D29) |
| pi-studio feature flag `daemonObservability` (stage 2) | capability | Health/logs panels show "daemon too old" when absent (D26) |
| pi-studio web UI URL per daemon entry: `<webUiUrl>/?host=<ws url>&connect=1#agent=<agentId>` | external UI | "Open in web UI" (D24) |
| OS keychain service `com.avatsaev.pistudiodashboard` | secret store | Daemon passwords (D18) |
| Application id `com.avatsaev.pistudiodashboard` | platform | Config dir, notifications, bundle id |
| `PISTUDIODASHBOARD_LOG` | env var | Dashboard's own log level (template) |

## Build / run / test / deploy overview

- Gate: `cargo xtask ci` (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm) must pass
  for every task.
- Run: `cargo run -p pistudiodashboard`; frontend dev loop `cd apps/pistudiodashboard && npm run app`.
- UI verification: `npm run snapshot` per theme (dark, light) — golden rule 5.
- Protocol verification: fixture tests over the vendored pi-studio fixtures (D22, D37); re-vendor
  when pi-studio's manifest `formatVersion` or `CHANGELOG.md` changes.
- Packaging: Tauri bundler, Linux (`.deb`, verified) and macOS (`.app`/`.dmg`, first run pending),
  unsigned (D34).
- pi-studio changes ship in pi-studio's own releases, additive and flag-advertised (D34).

## Sub-spec index

### Dashboard (this repo)

| File | Kind | Stage | Description |
|------|------|-------|-------------|
| [features/daemon-connections.md](features/daemon-connections.md) | feature | 1 | Add/edit/remove daemons, local auto-detect, connection states, auth/host errors, duplicates, degraded banners, diagnostics |
| [features/agent-monitor.md](features/agent-monitor.md) | feature | 1 | Card grid across daemons, filters, ordering, usage, live drill-down |
| [features/attention-and-interventions.md](features/attention-and-interventions.md) | feature | 1 | Attention rail, answering `confirm`/`select` dialogs, interrupt, open in web UI, races |
| [features/notifications-and-tray.md](features/notifications-and-tray.md) | feature | 1 | OS notifications (content-free, coalesced), click-to-focus, tray, close-to-tray |
| [features/daemon-health-and-logs.md](features/daemon-health-and-logs.md) | feature | 2 | Health panel and live log tail over the stage 2 daemon API |
| [architecture/app-structure.md](architecture/app-structure.md) | architecture | 1 | Crates and layers, threads, command registry, UiState, Services hooks, webview boundary, decision record |
| [architecture/protocol-client.md](architecture/protocol-client.md) | architecture | 1 | Wire types, connection state machine, handshake, sync, outbound allowlist, caps, fixtures |
| [architecture/live-state.md](architecture/live-state.md) | architecture | 1 | `MonitorState`, reducers, derivations, bounds, ingest batching, view coalescing |
| [architecture/config-and-secrets.md](architecture/config-and-secrets.md) | architecture | 1 | Config file, `.bak` recovery, settings, keychain |
| [architecture/design-system.md](architecture/design-system.md) | architecture | 1 | pi-studio tokens ported to `tokens.css`, themes, rebuilt components, snapshot checks |

### pi-studio (in `../pi-studio/swe/`, indexed in its `MAIN-SCOPE.md`; all shipped)

| File | Kind | Stage | Description |
|------|------|-------|-------------|
| [web-ui-agent-deep-link.md](../../../pi-studio/swe/features/web-ui-agent-deep-link.md) | feature | 1 | `?host=…&connect=1#agent=<agentId>` opens the web UI on that agent — shipped sprint-082 |
| [external-client-fixtures.md](../../../pi-studio/swe/architecture/external-client-fixtures.md) | architecture | 1 | Canonical fixtures of every message external clients consume, validated against a live daemon — shipped sprint-083 |
| [daemon-status-rpc.md](../../../pi-studio/swe/features/daemon-status-rpc.md) | feature | 2 | `daemon_status_*`: real version, uptime, CPU/mem, event-loop lag, agents by status, connected clients — shipped sprints 084–085 |
| [daemon-log-streaming.md](../../../pi-studio/swe/features/daemon-log-streaming.md) | feature | 2 | `daemon_logs_*`: backlog + live pino lines, minLevel/effectiveLevel, 200 lines/s cap — shipped sprint-085 |

## Open questions — TODO(verify)

- [ ] (brief, non-blocking) Today's workflow and how success is measured — owner: avatsaev.
- [x] pi-studio listen default — resolved: the daemon binds `127.0.0.1:6767` by default
      (pi-studio sprint-087, D35).
- [x] Tool-permission channel — resolved: retired in pi-studio sprint-086; removed from this scope
      (D37).
- [x] Dialog payload/response fields and the agent, timeline, interrupt and reset shapes —
      resolved by the shipped fixtures (`agent_ui_request.{confirm,select}.json`,
      `agent_ui_respond_*.json`, `list_agents_response.json`, `fetch_agent_timeline_*.json`,
      `interrupt_*.json`, `agent_timeline_reset.json`).
- [x] `list_agents_response` excludes archived agents (`AgentManager.list()` filters
      `archivedAt`); `agent_archived` removes a card — [architecture/protocol-client.md](architecture/protocol-client.md).
- [x] Stale-event detection across reconnects — decided D38: every `DaemonEvent` carries a
      per-daemon `connEpoch`; the reducer drops lower epochs ([architecture/live-state.md](architecture/live-state.md), [architecture/protocol-client.md](architecture/protocol-client.md)).
- [ ] Golden rule 9 licence checks: `keyring`, `tauri-plugin-notification`, tray feature, the
      WebSocket/TLS crates — [architecture/app-structure.md](architecture/app-structure.md).
- [ ] Tray/notification behaviour per platform (Linux click callbacks, macOS first run) —
      [features/notifications-and-tray.md](features/notifications-and-tray.md).
- [ ] Proposed numeric caps that are decisions of their specs (50 daemon entries, 64-char names,
      5,000 log lines, 2 KB payload strings, 100 options, 15 s respond timeout) — confirm or adjust.

Each sub-spec carries its own full `TODO(verify)` list.
