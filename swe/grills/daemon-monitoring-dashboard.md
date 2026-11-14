# Grill — Pi-Studio daemon monitoring dashboard

> Status: grilled
> Started: 2026-11-13   Last updated: 2026-11-13   Rounds: 8
> Engineer: avatsaev   Agent: omp (claude-opus-5-5)

## Raw idea
i want to create a monitoring dashboard to pi-studio daemon, i want to monitor everyting that is going on on a pi-stduio daemon server in real time in this application, the code source of pi-studio is located at @../pi-studio/ please reuse its design language here

## Codebase context
| Area | Path(s) | Fact that matters |
|------|---------|-------------------|
| Dashboard app (this repo) | `AGENTS.md`, `crates/*`, `apps/pistudiodashboard/` | Fresh av-rust-desktop scaffold (sample checklist domain, gate green). Rule 1: all network/state in Rust; TS frontend only draws the view. Rule 6: webview gets 2 commands + 1 event, strict CSP. Rule 7: core crates build for wasm32. Rule 9: permissive licences only (MIT/Apache). |
| Dashboard theming | `apps/pistudiodashboard/src/tokens.css` | Colours must come from tokens here; pi-studio's tokens would be ported into this file. |
| Pi-Studio overview | `../pi-studio/AGENTS.md`, `../pi-studio/swe/MAIN-SCOPE.md` | TS monorepo; daemon (Node) manages agents, PTY terminals, git worktrees, projects, chat rooms, schedules, loops; exposes one WebSocket JSON+binary API. Protocol is append-only, gated by capability flags. |
| Pi-Studio licence | `../pi-studio/package.json` | `AGPL-3.0`; engineer is the near-sole author. Copying tokens/CSS/assets into an MIT/Apache app needs an explicit relicensing decision. |
| Pi-Studio specs | `../pi-studio/swe/architecture/websocket-protocol.md`, `auth-security.md`, `relay-e2ee.md`, `design-system.md` | Canonical contract + design docs a Rust client would be built from. |
| Pi-Studio design system | `../pi-studio/packages/web-client/DESIGN_SYSTEM.md`, `src/theme/{colors,variants,tokens}.ts` | CSS Modules + runtime-emitted `--pi-*` CSS vars (no Tailwind, no hand-written tokens.css). Six themes (dark default, light, zinc, midnight, claude, ghostty); status tokens `statusSuccess/Danger/Warning/Info/Merged`; dense IDE look, colour reserved for state; pixel-keyed spacing, 13–14px base. |
| Pi-Studio reusable UI patterns | `src/components/primitives/StatusDot.tsx`, `StatusBadge`, `features/sessions/SessionList.module.css`, `features/chat/rows/rows.module.css` (tool cards), `StatusBar.module.css`, `ToastViewport` | Agent state → dot colour map already exists (`features/sessions/status-map.ts`: initializing→queued, running, error, closed→finished). No table component exists — dashboard needs its own. Icons: lucide-react (ISC); overlays: Radix (MIT). |
| Daemon transport | `packages/server/src/ws/ws-server.ts`, `ws/router.ts`, `packages/server/docs/rpc-communication.md`, `packages/protocol/src/endpoint.ts` | `ws(s)://host:6767`, any path. First frame `hello {clientId, clientType: mobile\|browser\|cli\|mcp, protocolVersion: 1, capabilities}` → `status/server_info {serverId, hostname, version, homeDir, features}`. RPC/push = `{type:"session", message:{type, requestId?}}`. App-level JSON ping/pong; server never pings. |
| Daemon auth | `http/host-allowlist.ts`, `auth/password-auth.ts` | Host-header allowlist (localhost/IPs always ok). Optional password sent as WS subprotocol `pi-studio.bearer.<pw>`. `GET /api/health` is unauthenticated. |
| Relay E2EE | `packages/relay`, `client/src/relay-transport.ts`, `client/src/pairing.ts`, `swe/architecture/relay-e2ee.md` | NaCl box (X25519+XSalsa20-Poly1305) + SHA-512 session id — easy in Rust (`crypto_box`, `sha2`). **Daemon holds one relay peer: "last hello wins"** — a monitor on the relay would kick the user's web UI off (and vice versa). Direct WS has no such limit. |
| Agents | `agent/agent-service.ts`, `protocol/src/messages.ts`, `docs/agent-stream-events.md` | Status `initializing\|idle\|running\|error\|closed`. `list_agents_request` (no cost/tokens). Pushes broadcast to **all** clients with no subscribe step: `agent_update`, `agent_status`, `agent_stream {seq, event}` (token deltas, tool_call, turn_*, compaction), permission/UI requests, todos. Tokens/cost only via poll `agent_session_stats_request` (live sessions only). No pid/cpu/mem. |
| Terminals | `terminal/terminal-manager.ts`, `terminal-rpc.ts` | `terminals_update` broadcast (slot, workspace, name, cwd, shell, cols, rows, closed). Bytes only to subscribers via binary frames; **`subscribe_terminal_request` resizes the PTY** — a monitor must not subscribe blindly. No pid, start time or exit code. |
| Projects / worktrees / git | `projects/`, `projects/worktree-service.ts`, `status-projection.ts` | `list_projects`, `list_workspaces` (synthesises a workspace per loose agent), `workspace_update` push. Git status per-cwd via `checkout_status_subscribe`. |
| Schedules / loops / chat | `orchestration/`, `daemon/orchestration-rpc.ts` | List/inspect/logs RPCs exist, **no push events** — polling only. Loops have cursor-based `loop_logs_request`. |
| Daemon-level observability | `logging/logger.ts`, `daemon/bootstrap.ts`, `protocol/src/client-capabilities.ts` | **None over the wire**: no metrics/uptime/mem/cpu, no connected-clients list, no log streaming (pino NDJSON at `$PI_STUDIO_HOME/logs/pi-studio.log`, local only), `version` hard-coded `"1.0.0"`, `daemonStatusRpc` feature flag advertised but unimplemented. |
| Replay / backpressure | `ws/session.ts`, timeline store | No global event cursor; reconnect = re-list + per-agent `fetch_agent_timeline_request {after: seq}`. No backpressure: a slow client makes the daemon buffer. |
| Protocol schema | `packages/protocol/src/messages.ts` | Zod only, no JSON Schema/codegen; many list/orchestration responses are untyped passthrough — Rust types must be lenient (`Option`, `#[serde(other)]`). |
| Mutations | various `*-rpc.ts` | No role gating: any authenticated client can call every mutation (interrupt, delete, kill terminal, approve permissions…). |

## Problem & outcome
- **Who:** the engineer (avatsaev), who runs one or more pi-studio daemons (local + LAN/remote) and drives agents on them.
- **Today:** TODO(verify) — current workflow and pain not stated in round 1.
- **Done means:** one native window shows, live and across several daemons, every agent's state and current activity, what needs attention, token/cost usage, daemon health (incl. connected clients and version) and a live log tail, and lets the engineer interrupt an agent or answer a permission request without opening the web UI.
- **Measured by:** TODO(verify).

## Goals
- Live view of all agents on each daemon: status and current activity (current tool call, streaming output). (D1)
- Attention feed: pending permission/UI requests, errors, failed turns. (D1)
- Token and cost usage per agent and in total. (D1)
- Daemon health: up/down, uptime, cpu/mem, connected clients, version. (D1, needs D4)
- Live daemon log tail. (D1, needs D4)
- Two interventions only: interrupt an agent, answer a permission request. (D2)
- Several daemons at once over direct WebSocket. (D3)
- New read-only observability API in the pi-studio daemon to supply what the protocol lacks. (D4)

## Non-goals
- Full control: anything beyond interrupt and permission answers (create/delete agents, prompts, kill terminals, git operations, schedule edits). (D2)
- Connecting via the E2EE relay in v1. (D3)
- Browsing past transcripts / agent history (stays in the web UI). (D6)
- Answering `agent_ui_request` dialogs (questions, selects, free text); shown only, with a hand-off to the web UI. (D7)
- Persisted history, usage-over-time charts. (D8)

## Happy path
1. **Launch.** The engineer opens Pi Studio Dashboard (or it is already running in the tray). The local daemon is added automatically from `$PI_STUDIO_HOME/config.json`'s listen address (default `127.0.0.1:6767`); remote daemons were added once by name, host:port, ws/wss and optional password. (D10)
2. **Connect.** For each daemon the Rust core opens a direct WebSocket (password as the `pi-studio.bearer.<pw>` subprotocol), sends `hello`, receives `server_info`, then rebuilds state: list agents, list pending permissions (new RPC, D12), fetch session stats. Each daemon shows a connection state.
3. **Overview.** A card grid shows one card per agent across all daemons, daemons as filter chips. Each card: status dot (pi-studio colours), title, daemon, model, current tool call, last output line, tokens/cost. (D11, D6)
4. **Live updates.** `agent_update` / `agent_status` / `agent_stream` pushes update cards in place; usage refreshes by polling `agent_session_stats_request`.
5. **Drill-down.** Selecting a card shows the live stream of that agent's current turn (assistant text, reasoning, tool calls with status). (D6)
6. **Something needs the engineer.** An `agent_permission_request` arrives: the card is flagged, the attention feed gains an item (tool name + `action` preview, agent, daemon), and an OS notification is shown (D9). Clicking the notification focuses the window on that item. (D14)
7. **Answer.** The engineer clicks one of the daemon-provided `responses[]` buttons; the dashboard sends `agent.permission.respond.request`; `agent_permission_resolved` clears the item everywhere. A free-text question offers "Open in web UI" instead. (D13)
8. **Interrupt.** On a running agent, Interrupt asks for confirmation, then sends `interrupt_agent`; the card returns to idle and its pending requests clear. (D14)
9. **Background.** Closing the window keeps the app in the system tray; notifications keep arriving. (D14)

## Decisions
| # | Theme | Question | Decision | Rationale | Round |
|---|-------|----------|----------|-----------|-------|
| D1 | problem | What must v1 answer at a glance? | Agents live (status + current activity); attention items (permission/UI requests, errors, failed turns); token & cost usage; daemon health (up/down, uptime, cpu/mem, connected clients, version); live log tail. | Engineer's pick. Terminals, orchestration (schedules/loops/chat) and git were not selected → Deferred. | 1 |
| D2 | problem | Observe only or act? | Read-only plus two interventions: interrupt agent, answer permission request. | Engineer's pick (agent recommended strictly read-only). Daemon has no role gating, so the dashboard's own command set is the only guard: no other mutation may be sendable. | 1 |
| D3 | problem | Which daemons? | Several daemons (local + LAN/remote) over direct WebSocket with optional password. No relay in v1. | Relay holds one peer per daemon ("last hello wins"): a monitor would displace the web UI. | 1 |
| D4 | problem | May this effort change pi-studio? | Yes: add a read-only observability API to the daemon (status/metrics, connected clients, log tail, real version; orchestration pushes only if orchestration comes back into scope). | Daemon health and logs are unreachable remotely otherwise. Makes this a two-repo effort. | 1 |
| D5 | scope | Delivery shape | Staged. **Stage 1:** dashboard against today's protocol: agents live, attention, usage, up/down via `GET /api/health`, interrupt + permission answers, notifications. **Stage 2:** new daemon observability API (metrics, connected clients, log tail, real version) + the dashboard's health and log panels. | Stage 1 is useful alone and doesn't wait on a pi-studio release; stage 2 is a cross-repo change. | 2 |
| D6 | scope | Depth of "current activity" | Overview row per agent (status dot, current tool call, last output line, tokens/cost) + a live drill-down of one selected agent's current turn. No transcript/history browsing. | Monitor, not a second web UI. | 2 |
| D7 | scope | Which "needs you" requests are answerable? | `agent_permission_request`: answered in the dashboard (allow/deny). `agent_ui_request`: shown in the attention feed with "Open in web UI", not answered. | UI requests are open-ended extension dialogs; rendering them all would re-build the web UI. | 2 |
| D8 | scope | History | Live only: state rebuilt from the daemon on every connect; in-memory ring of recent events/attention items per daemon (size TBD in data round). Nothing persisted except the daemon list/settings. | Smallest correct version; the daemon is the source of truth. | 2 |
| D9 | scope | OS notifications | Yes, for permission requests and failed turns/errors; each kind toggleable. | A monitor must be useful while in the background. | 2 |
| D10 | happy path | Adding daemons | Auto-detect the local daemon from `$PI_STUDIO_HOME/config.json` (default `127.0.0.1:6767`) + manual add (name, host:port, ws/wss, optional password). No pairing-link parsing. | Engineer's pick. | 3 |
| D11 | happy path | Main layout | Card grid: one card per agent across daemons; daemons as filter chips. Drill-down and attention feed placement within it: see open questions. | Engineer's pick (agent recommended the pi-studio three-column shell). | 3 |
| D12 | happy path | Late-join permission gap | Pull a small additive pi-studio change into **stage 1**: a WS RPC listing pending permissions (store already has `listForAgent`) and `questionKind` + `allowComment` added to the `agent_permission_request` broadcast. **Revises D5:** stage 1 now needs a pi-studio release too; stage 2 keeps metrics/clients/logs/version. | "What needs me" must be correct regardless of when the dashboard connected. | 3 |
| D13 | happy path | Answering requests | Render the daemon's `responses[]` as buttons (allow/deny, select options). `input`/`editor` questions and `allowComment` requests show "Open in web UI". Show tool name + `action` preview, agent, daemon. | Keeps the dashboard to button answers (D7 spirit); questionKind from D12 makes the split reliable. | 3 |
| D14 | happy path | Interrupt / notifications / tray | Interrupt needs confirmation (it also cancels that agent's pending permissions). A notification click focuses the window on the item. Closing the window keeps the app in the system tray (toggleable). | Interrupt cannot be undone; background notifications need a live process. | 3 |
| D15 | data | Card set & placement | Archived agents hidden. Order: needs-attention → running → error → idle → closed (dimmed). Status filter chips beside daemon chips. Attention feed = collapsible right rail; selecting a card opens the drill-down as a right panel that replaces the rail while open. | Engineer confirmed recommendation; closes the D11 open question. | 4 |
| D16 | data | Usage polling | Poll `agent_session_stats_request` every 10 s for running agents and once after `turn_completed`/`turn_failed`; idle agents once on connect. No live session → "—". Totals labelled "N of M agents". Cost shown as the daemon reports it, no conversion. | Bounded load; never pass a partial sum off as total. | 4 |
| D17 | data | Memory bounds | Per agent: derived summary only. Full current-turn stream only for the drilled-into agent (seeded by `fetch_agent_timeline_request`, then live), capped at 2,000 items. Attention/event ring: last 500 per daemon. | Every client receives every agent's deltas; holding them all is unbounded. | 4 |
| D18 | data | Passwords at rest | OS keychain (`keyring` crate). No keychain available → prompt at connect, never written to disk. Daemon list + settings in a JSON file in the app config dir. | No plaintext secrets on disk. | 4 |
| D19 | data | Daemon identity | Key by `serverId` from `server_info`. Two entries with one `serverId` → one daemon + "duplicate entry" warning. A new `serverId` at the same address = a new daemon; the old card set clears. | Prevents double-counting the local daemon added both ways. | 4 |
| D20 | integration | Design reuse & licence | Port pi-studio token *values* into `apps/pistudiodashboard/src/tokens.css` under the `--pi-*` names; rebuild the needed components (status dot, badge, card, rail, status bar, toast) from pi-studio's recipes in the dashboard frontend. No pi-studio source files copied. The engineer, as pi-studio's copyright holder, licenses the ported design values to the dashboard under MIT OR Apache-2.0; provenance recorded in `ATTRIBUTION.md`. | Keeps golden rule 9 intact without AGPL contagion. | 5 |
| D21 | integration | Themes | Dark (default) + light + follow-system. | Smallest set; other variants deferred. | 5 |
| D22 | integration | Protocol types | Hand-written, lenient serde types for only the messages used (unknown fields tolerated, unknown enum values → `Unknown`), in a new crate. Verified by fixtures recorded from a real daemon, plus a contract test in pi-studio that fails when a fixture's message shape changes. | Zod-only, passthrough-heavy protocol; codegen would be incomplete. | 5 |
| D23 | integration | `hello` identity | `clientType: "cli"` + `appVersion: "pistudiodashboard/<version>"`. | `clientTypeSchema` is a strict enum; a new value would lock out un-updated daemons. | 5 |
| D24 | integration | "Open in web UI" | Optional web UI URL per daemon entry (local default `http://localhost:4173`), opened in the system browser. Stage 1 pi-studio change adds an agent deep link to the web UI (e.g. `#agent=<agentId>` selects that session). | Web UI is served separately and has no deep link today. | 5 |
| D25 | edge cases | Connection failures | Unreachable → backoff 0.5 s ×2 to 30 s, 20% jitter; cards stay visible, dimmed, "stale since <time>". 401 → stop, "auth failed", prompt for password. 403 → stop, "add <host> to PI_STUDIO_HOSTNAMES or connect by IP". Restart with same `serverId` → silent rebuild. Mid-session drop → reconnect + full re-list (no replay). | Mirrors pi-studio's own client; never retry a credential error in a loop. | 6 |
| D26 | edge cases | Version skew | Detect stage 1/2 support by feature flags in `server_info`; missing → degraded mode with a per-daemon banner (late-join requests may be missing; without `questionKind` every request hands off to the web UI). Unknown message types / enum values ignored and counted, never fatal. | Works against every daemon in the field; honest about gaps. | 6 |
| D27 | edge cases | Races & stale actions | In-flight answer/interrupt disables its button. Resolved elsewhere (`resolved:false` or `agent_permission_resolved` first) → item clears, toast "answered elsewhere: <decision>". Notification for a resolved request → shows it was resolved. Interrupt on a non-running agent → no-op + message. Selected agent archived/deleted → drill-down closes with notice. `agent_timeline_reset` → drill-down refetch. >3 notifications in 5 s → one summary notification. | First-answer-wins daemon semantics; avoid notification storms. | 6 |
| D28 | edge cases | Scale & hostile input | View updates coalesced to ≤ 1 per 100 ms. Card grid virtualised above ~100 cards. Inbound frame cap 16 MiB (over → close + reconnect with error). Malformed JSON dropped + counted, connection kept. `action` preview truncated to 2 KB. Daemon timestamps shown in local time; "x ago" uses the local clock (skew accepted). Capacity target: 10 daemons, 500 agents. | Golden rule 2 (hostile input, bounded sizes); unfiltered delta broadcast. | 6 |
| D29 | edge cases | Local state & diagnostics | Corrupt config → saved as `.bak`, start empty, warning. Keychain locked/denied → prompt like a missing password. Unreadable local pi-studio `config.json` → `127.0.0.1:6767`. Per-daemon diagnostics panel: last error, reconnect count, messages/s, unknown/malformed counts, ping/pong RTT. Own log file; never logs passwords or permission `action` payloads. | Debuggability of the monitor itself. | 6 |
| D30 | non-functional | Stage 2 API shape | New feature flag `daemonObservability` (`daemonStatusRpc` left untouched). `daemon_status_request` → real version (package.json), startedAt, uptime, pid, Node version, RSS/heap, CPU %, event-loop lag, agents by status, connected clients (clientId, clientType, appVersion, connectedAt, remote address, direct/relay). `daemon_status_subscribe` → `daemon_status` push every 5 s to subscribers only. `daemon_logs_subscribe {minLevel, tail ≤ 1000}` → `daemon_log_lines` pushes, capped server-side at 200 lines/s with a dropped-lines counter. | Subscribe-only keeps the web UI's traffic unchanged; caps protect the daemon. | 7 |
| D31 | non-functional | Access to logs/clients | No extra gate: observability RPCs are available to any authenticated client, like every other RPC. | Engineer's pick; upheld after the round 8 challenge (see D35): an open daemon is already fully controllable, so the gate would not close the real hole. | 7 |
| D32 | non-functional | Client security & privacy | Plain `ws://` + password to a non-loopback host allowed, with an "unencrypted" badge; `wss://` supported. Notifications carry agent title + daemon + tool name only (no command/path content). Passwords never reach the webview. | Lock-screen safe; honest about plaintext. | 7 |
| D33 | non-functional | Performance budgets | At 10 daemons / 500 agents: dashboard idle CPU < 2%, RSS < 300 MB; permission request visible < 500 ms after daemon broadcast; stage 2 status sampling < 1% daemon CPU. | Become acceptance criteria. | 7 |
| D34 | non-functional | Platforms & rollout | Linux + macOS in v1 (macOS needs a first manual run); Windows deferred. Unsigned builds for personal use; signing/notarization deferred. pi-studio changes ship additive and flag-advertised in normal releases; dashboard rollback never needs a daemon rollback. | Engineer's platforms; the template is verified only on Linux. | 7 |
| D35 | non-functional | Challenge to D31: `daemon/main.ts` defaults `listen` to `0.0.0.0:6767` (docs/config say `127.0.0.1:6767`), no password by default | D31 stands. Aligning the daemon's default listen address to `127.0.0.1` is a **separate pi-studio bug fix, outside this effort**. | The exposure exists today independently of the dashboard. | 8 |
| D36 | scope-time revision (supersedes parts of D2, D7, D12, D13, D14) | Evidence found while scoping: no production code raises `agent_permission_request` (`PermissionService.requestPermission` has no non-test caller; Pi adapter `getPendingPermissions()` returns `[]`; `interrupt_agent` does not call `cancelPending`; the web UI has no permission UI). Pi's questions arrive as `agent_ui_request` with `expectsResponse: true` (`select`/`confirm`/`input`/`editor`), answered by `agent_ui_respond_request`, late-join via existing `agent_ui_list_request`; interrupt leaves them pending (`swe/features/extension-ui-rpc.md` in pi-studio). | "Answer what needs me" targets Pi dialogs: `confirm` (yes/no) and `select` (option buttons) are answered in the dashboard; `input`/`editor` hand off to the web UI. The permission channel is still displayed and answered if a daemon ever emits it, but **no pi-studio permission work** (the late-join RPC + `questionKind` change of D12 is dropped). Daemon mutations allowed: `agent_ui_respond_request`, `interrupt_agent`, `agent.permission.respond.request`. Stage 1 pi-studio changes shrink to the web UI deep link and the fixtures contract. | Engineer's choice after the evidence; makes the interventions useful against real daemons today. | scope |
| D37 | post-scope sync (supersedes the dormant-permission parts of D36) | pi-studio shipped its side on 2026-11-13 (`47d80e6`): sprint-082 deep link, 083 external-client fixtures (`packages/protocol/fixtures/external-clients/`, `formatVersion` 1), 084/085 `daemonObservability` status + logs, 086 **tool-permission family retired** (service, handlers, MCP tools and CLI removed; wire schemas kept unread; no fixtures), 087 daemon binds `127.0.0.1:6767` by default (D35 resolved). | The dashboard drops the permission channel entirely: no `permission.respond` command, no permission decoding, no `agent.permission.respond.request` in the allowlist. Daemon mutations allowed: exactly `agent_ui_respond_request` and `interrupt_agent`. Dashboard specs are aligned to the shipped fixtures as the wire contract. | A consumer of a retired family is dead code; sending it would now return `rpc_error unknown_message_type`. | sync |
| D38 | post-scope (edge cases) | How does the reducer drop an event left over from an earlier connection after a reconnect? | Every `DaemonEvent` carries `connEpoch: u64`, a per-daemon-entry counter the L4 client increments when it starts each new connection attempt. The reducer remembers the newest epoch per daemon (set by that attempt's first `ConnState`) and drops any event with a lower epoch. | Engineer's call ("tag it"); `seq` alone restarts per connection and cannot order across reconnects. | sync |
| D39 | plan-time (architecture) | Who decides when stats and drill-down timelines are fetched — engine effects or the client? (the specs described both) | The **engine** is the only scheduler: `FetchStats` effects on `Snapshot`, turn end, new agent, plus an owner-loop `stats_tick` every 10 s for running agents; `FetchTimeline` effects on select, `agent_timeline_reset` and reconnect. The **client** only executes: dedupes stats per agent (≤ 4 in flight), performs timeline paging (`before`, then `after` while `hasNewer`, ≤ 2,000 items), and never fetches on its own. | One owner per decision avoids double requests and keeps triggers testable as pure reducer output. Agent's call while planning. | plan |

## Data & state
**Persisted (app config dir, JSON, owned by the dashboard):**
- Daemon entries: `{ entryId, name, host, port, tls, lastSeenServerId?, autoDetected }`. Password never in this file (D18).
- Settings: notification toggles per kind (permission requests, failures) (D9), close-to-tray (D14), theme (round 5).

**Secrets:** daemon password per `entryId` in the OS keychain (D18).

**In memory only, rebuilt on every connect (D8):**
- Per daemon: connection state (connecting / connected / auth failed / unreachable / reconnecting), `server_info` (`serverId`, hostname, version, features), last health result.
- Per agent (keyed `serverId` + `agentId`): status, title, cwd, labels, provider/model, lastActivity, derived activity summary (current tool call, last output line), usage snapshot `{tokens, cost, contextUsage}` + when it was taken or "—" (D16), attention flags.
- Pending permission requests (keyed `serverId` + `requestId`): agent, toolName, action preview, `responses[]`, questionKind, allowComment (D12, D13). Removed on `agent_permission_resolved`.
- Drill-down buffer for the one selected agent, ≤ 2,000 items (D17).
- Attention/event ring, 500 per daemon (D17).

**Derived, never stored:** card order, status counts per daemon, usage totals with coverage ("N of M").

**Owned by the daemon, never mirrored to disk:** agents, timelines, permissions. Stage 2 adds daemon metrics, clients, logs (D4/D5).

## Integration points
| Existing module / service / config | How the feature touches it | Direction | Risk / note |
|------------------------------------|----------------------------|-----------|-------------|
| `crates/*` layering (`xtask/src/layers.rs`) | New crates: protocol types/codec (pure, wasm-safe) and a daemon-connection layer (WebSocket, timers). Sample checklist domain is replaced. | writes | Network I/O must stay out of wasm-checked crates or behind `Services` (golden rule 7); register every new crate in `layers.rs`. |
| `crates/shell/src/state.rs` + `ui.inspect` | The card grid, rail, drill-down and status bar are one serde view struct. | writes | Golden rule 5: the frontend draws only this; snapshots verify it. |
| `crates/engine/src/*_cmds.rs` | Commands: add/edit/remove daemon, connect/disconnect, select agent, filter, answer permission, interrupt agent, open in web UI, toggle notifications/tray, theme. | writes | Golden rule 3. The two mutations (D2) are the only daemon-mutating commands; a test must assert no other daemon mutation can be sent. |
| `apps/pistudiodashboard/src-tauri` | Tray icon (Tauri `tray-icon` feature), OS notifications (notification plugin used from Rust only), keychain (`keyring`), config dir, opening URLs in the system browser. | writes | Golden rule 6: none of these is granted to the webview; licences of `keyring`/notification plugin to be checked (rule 9). |
| `apps/pistudiodashboard/src/tokens.css` | Ported `--pi-*` tokens, dark + light (D20, D21). | writes | Snapshot check per theme. |
| pi-studio `packages/server/src/ws/*`, `ws/router.ts` | WebSocket + `hello`/`server_info` handshake, session envelope, JSON ping/pong. | consumes | Direct WS only (D3). Password via `pi-studio.bearer.<pw>` subprotocol. Host allowlist: connect by IP/localhost or an allow-listed name. |
| pi-studio `GET /api/health` | Up/down probe before and between WS attempts. | consumes | Unauthenticated; says nothing about auth validity. |
| pi-studio agents (`list_agents_request`, `agent_update`, `agent_status`, `agent_stream`, `agent_archived`, `agent_deleted`, `agent_timeline_reset`, `fetch_agent_timeline_request`, `agent_session_stats_request`) | Card grid, drill-down, usage. | consumes | Unfiltered broadcast of all deltas (D17). Stats need a live session (D16). |
| pi-studio permissions (`agent/permissions.ts`, `agent_permission_request/_resolved`, `agent.permission.respond.request`) | Attention feed + answers. | consumes + writes (respond) | First answer wins; another client may resolve first. |
| pi-studio `interrupt_agent` | Interrupt. | writes | Also cancels that agent's pending permissions. |
| pi-studio stage 1 change: pending-permissions WS RPC + `questionKind`/`allowComment` in the broadcast (D12) | Late-join correctness, answer/hand-off split. | pi-studio emits; dashboard consumes | Additive; new server feature flag so the dashboard can tell old daemons apart (see edge cases). |
| pi-studio stage 1 change: web UI agent deep link (D24) | "Open in web UI". | pi-studio web-client | Additive route/hash handling. |
| pi-studio stage 2 change: observability API (D4, D5) | Daemon health, clients, logs, real version. | pi-studio emits; dashboard consumes | Must use a **new** feature flag: `daemonStatusRpc` is already advertised with no handler. Shape decided in a later round. |
| pi-studio `agent_ui_request` / `agent_ui_list_request` | Shown in attention feed with hand-off (D7). | consumes | `agent_ui_list_request` exists for reconnect catch-up. |
| pi-studio `$PI_STUDIO_HOME/config.json` (local) | Auto-detect local listen address (D10). | reads | File may be absent or unreadable; default `127.0.0.1:6767`. |
| pi-studio protocol contract tests | Fixture-backed contract test (D22). | pi-studio test suite | Keeps the Rust types honest across pi-studio releases. |

## Edge cases & failure modes
| Area | Condition | Decided behavior | Decision |
|------|-----------|------------------|----------|
| inputs | Malformed JSON frame from a daemon | Dropped, counted in diagnostics, connection kept | D28 |
| inputs | Frame larger than 16 MiB | Connection closed with an error, reconnect with backoff | D28 |
| inputs | Huge `action` payload in a permission request | Preview truncated to 2 KB | D28 |
| inputs | Unknown message type / unknown enum value (newer daemon) | Ignored and counted; unknown status renders as "unknown" | D26 |
| inputs | Manual daemon entry with bad host/port | Rejected at entry with the reason (port 1–65535, non-empty host) | D10 |
| identity & access | No/wrong password (401) | Stop retrying, "auth failed", prompt | D25 |
| identity & access | Host header not allow-listed (403) | Stop, explain `PI_STUDIO_HOSTNAMES` or connect by IP | D25 |
| identity & access | Keychain locked or access denied | Prompt for the password; nothing written to disk | D29 |
| identity & access | Dashboard could send any mutation (no role gating in daemon) | Only `agent.permission.respond.request` and `interrupt_agent` are sendable; tested | D2 |
| state | Same daemon added twice (local auto + manual) | One daemon by `serverId`, "duplicate entry" warning | D19 |
| state | Daemon reinstalled: new `serverId` at the same address | Treated as a new daemon; old cards cleared | D19 |
| state | Selected agent archived or deleted | Drill-down closes with a notice | D27 |
| state | `agent_timeline_reset` (fork) on the selected agent | Drill-down refetched | D27 |
| state | Agent recovered after daemon restart has no live session | Usage shows "—"; totals "N of M" | D16 |
| timing | Permission answered by another client first | Item clears, toast "answered elsewhere: <decision>" | D27 |
| timing | Double click / repeated answer | Button disabled while in flight; daemon is first-answer-wins anyway | D27 |
| timing | Interrupt on an agent that already stopped | No-op + message | D27 |
| timing | Dashboard starts after a permission request was raised | Listed via the stage 1 RPC; on old daemons a degraded banner | D12, D26 |
| timing | Mid-session disconnect | Reconnect with backoff, full re-list; no replay exists | D25 |
| timing | Notification storm | >3 in 5 s → one summary notification | D27 |
| timing | Clock skew daemon vs dashboard | Absolute times from the daemon in local time zone; relative times use the local clock, skew accepted | D28 |
| scale | Token deltas for every agent pushed unfiltered | Per-agent summary only; view coalesced to ≤ 1 / 100 ms | D17, D28 |
| scale | Hundreds of agents | Grid virtualised above ~100 cards; target 10 daemons / 500 agents | D28 |
| scale | Stats polling load | Running agents every 10 s + after each turn; idle once | D16 |
| dependencies | Daemon unreachable | Backoff; cards dimmed "stale since" | D25 |
| dependencies | Daemon predates stage 1 / stage 2 | Degraded mode by feature flag, per-daemon banner | D26 |
| dependencies | `daemonStatusRpc` flag advertised but unimplemented | Ignored; stage 2 uses a new flag | Integration |
| dependencies | Protocol shape drift across pi-studio releases | Lenient types + fixture contract test in pi-studio | D22 |
| data lifecycle | Dashboard config file corrupt | Kept as `.bak`, start empty, warning | D29 |
| data lifecycle | Local pi-studio `config.json` missing/unreadable | Default `127.0.0.1:6767` | D29 |
| data lifecycle | Removing a daemon entry | Its keychain password is deleted too | D18 |
| data lifecycle | Existing data predating the feature / migration | n/a — fresh app, live-only state, nothing to migrate | D8 |
| observability & ops | Why is a daemon red? | Per-daemon diagnostics panel (last error, reconnects, msgs/s, unknown/malformed counts, RTT) | D29 |
| observability & ops | Secrets in logs | Passwords and permission `action` payloads never logged | D29 |
| observability & ops | Feature flag off / rollback | n/a for the dashboard (separate app); pi-studio changes are additive and flag-advertised | D26 |

## Non-functional constraints
| Concern | Constraint / decision |
|---------|-----------------------|
| Performance / volume | Budgets D33. View coalescing ≤ 1 / 100 ms, virtualised grid, frame cap 16 MiB, bounded buffers (D17, D28). Stage 2 log stream capped at 200 lines/s (D30). |
| Security & authorization | Direct WS + optional password via bearer subprotocol; passwords in OS keychain (D18), never in the webview (D32). Only two daemon mutations are sendable (D2). "Unencrypted" badge for plain ws + password to non-loopback (D32). Stage 2 data available to any authenticated client (D31); the daemon's `0.0.0.0` listen default is tracked as a separate pi-studio fix (D35). Webview: two commands + one event, strict CSP (golden rule 6). |
| Privacy | Notifications without command/path content (D32). Logs never contain passwords or `action` payloads (D29). Live-only state, nothing about agents persisted (D8). |
| Backward compatibility | Works against every daemon in the field via feature flags + degraded mode (D26). `hello` uses existing `clientType: "cli"` (D23). pi-studio changes strictly additive (D34). |
| Rollout / feature flag | pi-studio: `daemonObservability` (stage 2) + a stage 1 flag for pending-permissions/questionKind (name chosen in scope). Dashboard: Linux + macOS, unsigned (D34). |
| Observability | Per-daemon diagnostics panel + own log file (D29). |
| Rollback | Dashboard is a separate app: uninstall/downgrade with no daemon change. pi-studio additions are inert unless a client calls them. |

## Deferred
- Terminals view (list + live output) — not selected in round 1 (D1). Note: `subscribe_terminal_request` resizes the PTY, so a later monitor must subscribe at the current size.
- Schedules, loops, chat rooms — not selected in round 1 (D1).
- Projects / worktrees / git status — not selected in round 1 (D1).
- Relay connectivity — deferred (D3); needs multi-peer relay support in the daemon first.
- Persisted event history and usage-over-time charts (D8).
- Answering extension UI requests in the dashboard (D7).
- Theme variants zinc, midnight, claude, ghostty (D21).
- A dedicated `clientType: "monitor"` (D23).
- Zod → JSON Schema export and generated Rust types (D22).
- Windows support; code signing and macOS notarization (D34).

## Open questions — TODO(verify)
- [ ] (non-blocking) Today's workflow and pain (what the dashboard replaces) and how success is measured — owner: avatsaev. Left blank in rounds 1 and 8; does not change the happy path, data model or contracts.
- [ ] (non-blocking, outside this effort) pi-studio bug: `packages/server/src/daemon/main.ts` defaults `listen` to `0.0.0.0:6767`, contradicting `config/daemon-config.ts`, `daemon/index.ts` and the docs (`127.0.0.1:6767`) — owner: avatsaev (D35)
- [ ] (non-blocking) Exact name of the stage 1 pi-studio feature flag and of the pending-permissions RPC — owner: `scope` (D12, D26)
- [ ] (non-blocking) Licences of `keyring`, the Tauri notification plugin and any WebSocket crate pass golden rule 9 — owner: `scope` (Integration)

## Acceptance criteria (draft)
> **Revised by D36 (scope time):** wherever an item below says "permission request", read "Pi dialog (`agent_ui_request`, `expectsResponse: true`)": `confirm`/`select` are answered via `agent_ui_respond_request`, `input`/`editor` hand off; late-join uses the existing `agent_ui_list_request` (no new pi-studio RPC, no stage 1 flag); interrupt leaves pending dialogs in place. The specs under `swe/specs/` carry the corrected criteria.

**Stage 1**
- [ ] Given a local daemon at its default address, when the dashboard launches for the first time, then it connects with no input and non-archived agents appear as cards within 2 s. (D10, D15)
- [ ] Given a connected daemon, when an agent starts a turn and makes tool calls, then its card shows them within 500 ms of the daemon broadcast; usage shows tokens/cost and totals read "N of M agents". (D6, D16, D33)
- [ ] Given a stage 1 daemon with a permission request raised before the dashboard connected, when the dashboard connects, then the request is in the attention rail. (D12)
- [ ] Given a pending request, when the engineer clicks one of its `responses[]` buttons, then the daemon resolves it and it clears in the dashboard and the web UI. (D13)
- [ ] Given a pending request answered first in the web UI, then the dashboard item clears with "answered elsewhere: <decision>". (D27)
- [ ] Given an `input`/`editor` question, then no answer buttons are shown and "Open in web UI" opens `<url>#agent=<id>` in the system browser on that agent. (D13, D24)
- [ ] Given a running agent, when the engineer interrupts and confirms, then the agent goes idle and its pending requests clear. (D14)
- [ ] Given the window closed to the tray, when a permission request arrives, then an OS notification shows agent title, daemon and tool name only, and clicking it focuses the item. (D9, D14, D32)
- [ ] Given a wrong password, then the daemon shows "auth failed" with no retry loop; given a 403, the message names `PI_STUDIO_HOSTNAMES`. (D25)
- [ ] Given a connected daemon, when it is killed, then its cards dim with "stale since"; when it restarts, state is rebuilt with no user action. (D25)
- [ ] Given a daemon without the stage 1 flag, then a degraded banner shows and everything else works. (D26)
- [ ] A test proves only `agent.permission.respond.request` and `interrupt_agent` can be sent as daemon mutations. (D2)
- [ ] Given a synthetic load of 10 daemons × 50 streaming agents, then idle CPU < 2%, RSS < 300 MB and ≤ 10 view updates/s. (D28, D33)
- [ ] Dark and light snapshots render from `--pi-*` tokens whose values match pi-studio's. (D20, D21)

**Stage 2**
- [ ] Given a stage 2 daemon, the daemon panel shows the real version, uptime, CPU/mem, event-loop lag and connected clients, including the dashboard itself by its appVersion tag. (D23, D30)
- [ ] Log tail: new lines appear within 1 s, the minLevel filter works, and above 200 lines/s a dropped-lines counter shows. (D30)
- [ ] Status sampling costs the daemon < 1% CPU. (D33)

## Round log
| Round | Theme | Summary |
|-------|-------|---------|
| 1 | problem & outcome | v1 must-haves: agents live, attention, usage, daemon health, logs. Read-only + interrupt/permission answer. Multi-daemon direct WS, no relay. Pi-studio daemon gains an observability API. Today/measure left open. |
| 2 | scope boundary | Staged delivery (today's protocol first, daemon API second). Overview + live drill-down, no history browsing. Permissions answerable, UI requests hand-off. Live-only state. OS notifications for permissions/failures. |
| 3 | happy path | Local auto-detect + manual daemons. Card-grid layout. Pending-permission RPC + questionKind pulled into stage 1 (revises D5). Button answers from `responses[]`, free text hands off. Confirmed interrupt, notification focus, tray. |
| 4 | data & state | Attention right rail, drill-down panel replaces it. Bounded polling of stats with "N of M" totals. Summary-only per agent, full stream for the selected agent (≤2,000), 500-item event ring. Passwords in the OS keychain. Daemons keyed by serverId with duplicate warning. |
| 5 | integration points | Port tokens + rebuild components, engineer licenses design values as MIT/Apache. Dark/light/system. Hand-written lenient Rust types + fixtures + pi-studio contract test. hello as "cli" + appVersion tag. Per-daemon web UI URL + stage 1 deep link. Integration table filled. |
| 6 | edge cases | All proposals accepted: backoff + stop on 401/403, degraded mode by feature flag, race handling + notification coalescing, 100 ms view coalescing / 16 MiB cap / 10×500 target, corrupt-config recovery + diagnostics panel. Checklist walked: every area has rows. |
| 7 | non-functional | Stage 2 API: status request + 5 s subscribe push + capped log stream under new flag `daemonObservability`. No extra access gate (challenged next round). ws:// warning badge, content-free notifications. Numeric budgets accepted. Linux + macOS, unsigned. |
| 8 | acceptance | 17 acceptance criteria accepted as drafted (14 stage 1, 3 stage 2). D31 challenged with the `0.0.0.0` listen-default evidence; D31 upheld, default fix recorded as a separate pi-studio bug (D35). "Today / measured by" stays a non-blocking TODO. |
| scope | revision | Spec-time evidence: tool-permission channel is dormant in pi-studio. Engineer re-pointed answering to Pi `agent_ui_*` dialogs (D36); pi-studio permission work dropped. |
| sync | pi-studio shipped | pi-studio stage 1 + 2 shipped, permission family retired, listen default fixed. D37: permission channel removed from the dashboard; specs aligned to the shipped fixtures. |
| sync | edge cases | D38: tag every `DaemonEvent` with a per-daemon `connEpoch`; the reducer drops events from older connections. |
| plan | architecture | D39: engine decides when stats/timelines are fetched; client executes, dedupes and pages. |

## Handoff to scope
> **Scoped 2026-11-13.** The final file set differs from the table below (finer split by capability, and D36 dropped the pi-studio permission work): see the sub-spec index in [../specs/overview.md](../specs/overview.md).

Two repositories. Dashboard specs go under this repo's plan root (`swe/specs/`); pi-studio changes are specified in pi-studio's own plan root (`../pi-studio/swe/features/`, `swe/architecture/`, per its `av-swe.config.json`), so each repo's `plan`/`implement` stays self-contained.

| Spec file to produce | Fed by brief sections |
|----------------------|-----------------------|
| `swe/specs/overview.md` (dashboard) | Problem & outcome, Goals, Non-goals, staged delivery (D5, D12), Deferred |
| `swe/specs/features/agent-monitor.md` | Happy path 1–5, D6, D11, D15, D16, D17, D28, edge cases (state, scale), acceptance 1–2, 13 |
| `swe/specs/features/attention-and-interventions.md` | Happy path 6–9, D2, D7, D9, D12–D14, D24, D27, D32, edge cases (timing, identity), acceptance 3–8, 12 |
| `swe/specs/features/daemon-connections.md` | D3, D10, D18, D19, D23, D25, D26, D29, edge cases (dependencies, identity, data lifecycle), acceptance 1, 9–11 |
| `swe/specs/features/daemon-health-and-logs.md` (stage 2) | D4, D30, D31, D33, acceptance 15–17 |
| `swe/specs/architecture/protocol-client.md` | Codebase context (transport, auth, agents, permissions), D22, D23, D28, Integration points (pi-studio rows) |
| `swe/specs/architecture/app-structure.md` | Integration points (dashboard rows), Data & state, golden rules 1–9 constraints, D18, D34 |
| `swe/specs/architecture/design-system.md` | Codebase context (design rows), D20, D21, acceptance 14 |
| `../pi-studio/swe/features/monitor-support.md` (stage 1, pi-studio) | D12 (pending-permissions RPC, `questionKind`/`allowComment`), D24 (web UI agent deep link), D22 (fixture contract test), D26 (feature flag) |
| `../pi-studio/swe/features/daemon-observability.md` (stage 2, pi-studio) | D30, D31, D33, acceptance 15–17 |
