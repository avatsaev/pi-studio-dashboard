# Feature — Agent Monitor (card grid, filters, drill-down, usage)

> Part of: [overview.md](../overview.md)
> Dependencies: [live-state.md](../architecture/live-state.md), [attention-and-interventions.md](attention-and-interventions.md), [daemon-connections.md](daemon-connections.md), [design-system.md](../architecture/design-system.md), [app-structure.md](../architecture/app-structure.md)
> Source brief: [daemon-monitoring-dashboard.md](../../grills/daemon-monitoring-dashboard.md) (D1, D6, D11, D15, D16, D17, D26–D28, D33, D36, D37; happy path 3–5; acceptance 1, 2, 13)

## Purpose
The main window: one card per live agent across all connected daemons, with status, current activity and usage at a glance, filterable by daemon and status, plus a live drill-down of the one selected agent's current turn. A monitor, not a second web UI: no transcript browsing, no prompts (D6, D2).

## Public contract

### Commands (registry; camelCase `<area>.<verb>`; all go through the engine, golden rule 3)
| Command id | Args | Effect | Enabled when / reason when disabled |
|------------|------|--------|-----------------------------------|
| `agent.select` | `{ serverId, agentId }` | Sets `UiState.selection`; opens drill-down panel (replaces rail, D15); engine seeds `MonitorState.drill` and emits `FetchTimeline` | agent exists and not archived; else disabled "agent not available" |
| `agent.deselect` | `{}` | Clears selection; panel closes, rail returns; drill buffer freed | selection present |
| `view.filterDaemons` | `{ serverIds: string[] }` (empty = all) | Replaces the daemon filter (shell command, `UiState.filterDaemons`) | always; unknown ids ignored |
| `view.filterStatuses` | `{ statuses: AgentStatus[] }` (empty = all; `UiState.filterStatuses`) | Replaces the status filter | always |
| `view.toggleAttentionRail` | `{}` | Collapses/expands the rail (state only; collapsed rail still shows count badge) | no drill-down open (while drill is open, rail is hidden; command is a no-op with reason "drill-down open") |

Status chips are the five `AgentStatus` values (`initializing`, `idle`, `running`, `error`, `closed`); "needs attention" is not a filter value, it is a card flag and a sort rank (live-state `needs_attention`).

### View shape (L6 `UiState` + view; the frontend draws only this, golden rule 5)
```
view.monitor = {
  daemonChips: [{ entryId, name, conn: ConnState, staleSince?, selected: bool, agentCount }],
  statusChips: [{ bucket, count, selected: bool }],            # counts respect the daemon filter
  totals:      { tokens?, cost?, covered, of, label: "N of M agents" },
  cards:       { total: n, virtualised: bool, window: [Card] }, # window = visible slice (see Virtualisation)
  drill?:      { key, header: Card, items: [DrillItem], truncatedAtStart, state: loading|live|refetching, stale: bool },
  railCollapsed: bool
}
Card = { serverId, agentId, daemonName, title, model?, status, dot: DotKind, dimmed: bool, stale: bool, attention: {pendingDialogs, errored},
         currentTool?: { label, status }, lastOutputLine?, usage: { tokens?, cost?, contextPercent?, takenAgo?: string } | none,
         selected: bool }
```
`DotKind` = `queued|idle|running|waiting|finished|error` as in pi-studio's status dot vocabulary.

### Card fields (D6)
Status dot (pi-studio colours via `--pi-*` tokens), title, daemon name, model, current tool call, last output line, tokens/cost. Optional small badges: pending-request count, "stale" label (D25), "unencrypted" is a daemon-level badge in the chip (D32), not on the card.

## Behavior & algorithms

### Status dot mapping (reuse pi-studio semantics)
Source of truth: pi-studio `packages/web-client/src/features/sessions/status-map.ts` (protocol status → dot status) and `ui/status-dot.ts` (dot status → colour token; attention overrides).
```
protocol → dot:  initializing→queued   idle→idle   running→running   error→error   closed→closed (dashboard-only, see below)   Unknown→idle (label "unknown")
dot → colour token (status-dot.ts):
   running, queued → accent
   idle            → foregroundMuted (shown: the dashboard always shows inactive dots, i.e. showInactive = true)
   closed          → foregroundMuted (NOT pi-studio's finished→statusSuccess: a dimmed green dot reads as healthy; design-system.md § status dot)
   error           → statusDanger
   waiting         → statusWarning
   attention override (needs_attention):
       a pending dialog (`UiRequestItem`, D36)                                                   → statusWarning
       errored (agent status error or failed turn)         → statusDanger
```
Override precedence: pending dialog (warning) > errored (danger) > base status. The card's attention flag comes from pending dialogs (`attention.pendingDialogs`, D36, D37; no permission flag exists). `closed` agents render with a muted dot **and** a dimmed card (D15; design-system.md is the source for colours). Colour is never the only signal: the card also carries a text status word for accessibility.

Status sources (D37): the base status comes from `list_agents_response.agents[].status` at connect and from `agent_update.status` afterwards (partial updates: absent = unchanged). `agent_status` is never emitted by daemons and is not relied on. `lastActivity` (epoch ms) comes from the list; `agent_stream.timestamp` and `agent_archived.archivedAt` are ISO-8601 strings. Archived agents never appear in the list; an `agent_archived` push removes the card.

### Card grid
- One card per `AgentEntry` where `!archived` and its daemon is not a duplicate entry (D15, D19). Archived agents are hidden entirely; an `agent_archived` push removes the card (D15).
- Closed agents are shown, **dimmed**, ordered last. Stale agents (daemon disconnected) are dimmed and show "stale since <local time>" (D25); the dimming from `closed` and `stale` is the same visual treatment, stale additionally shows the label.
- Order: `card_order()` from [live-state.md](../architecture/live-state.md) — needs-attention → running → error → idle → closed; stable total order that does **not** reshuffle on stream deltas.
- Live updates modify cards in place (status, tool, last line, usage); changes are visible ≤ 500 ms after the daemon broadcast (D33, acceptance 2).
- Card text is plain text (never HTML/markdown rendered from daemon strings; hostile input, D28); lines truncated with ellipsis in the frontend, the model has already length-capped them.
- Relative times ("x ago") use the local clock; absolute times are local timezone (D28).

### Filters
- Daemon chips: one per configured daemon (including disconnected ones, showing connection state and a degraded-mode indicator per D26). Click toggles membership; empty selection = all daemons (`view.filterDaemons`).
- Status chips: `initializing`, `idle`, `running`, `error`, `closed` (only those with agents may be hidden), each with a count (computed over daemon-filtered cards, archived excluded) (`view.filterStatuses`). Empty selection = all.
- Filters AND together; within a chip row they OR. Filters affect cards, the totals and the status counts; they do **not** affect the attention rail (the rail always shows everything needing the engineer, because filtering it would hide a pending dialog).
- Filter state lives in `UiState`, not persisted in v1 [INFERENCE: brief persists only daemon list, notification toggles, close-to-tray, theme (Data & state)].
- If the filtered result is empty: empty-state message distinguishing "no agents" from "filters hide everything" (with a clear-filters action = `view.filterDaemons {}` + `view.filterStatuses {}`).

### Virtualisation (D28)
The shell always computes the full ordered card list (cheap: ids only). When `total > 100` (`VIRTUALISE_ABOVE = 100`), the view carries only the cards the frontend asks for: the frontend reports its visible index range (derived from scroll position and card height) through `view.setGridWindow {start, count}` (internal, no menu; stored as `UiState.gridWindow`, app-structure), plus an overscan of ±20. At ≤ 100 cards the whole list is sent. The backend never serialises more than ~200 cards per emit, bounding view size at 10 × 500 agents.

### Drill-down panel (D6, D15, D17, D27)
- `agent.select` opens a right-hand panel that **replaces** the attention rail while open; `agent.deselect` (close button or Esc) restores the rail (D15). Selecting another card swaps the buffer (the previous one is freed).
- Content, newest at the bottom, auto-follow unless the user scrolled up:
  - assistant text (streaming; plain text, plus a minimal render [TODO(verify): markdown rendering scope; default = plain text with preserved newlines]),
  - reasoning (collapsed by default, labelled "thinking"),
  - tool calls: kind icon, label (`toolDetailText`: command / path / query / url / description), status (`started|running|completed|error|canceled`), live output tail (last 12 lines) while running; completed calls show a one-line result summary only,
  - turn markers (started/completed/failed/canceled) and errors as rows.
  User messages and compaction are shown as compact system rows (not required to be rich).
- Seeding: on select, `fetch_agent_timeline_request` loads the current/most recent page (state `loading`), then live `agent_stream` events are folded in (state `live`). Events arriving while loading are held and de-duplicated by `seq` (algorithm in live-state).
- Cap 2,000 items; beyond that the oldest are dropped and a "earlier output not shown" row appears (`truncatedAtStart`) (D17). Not a history browser: no paging upward beyond what was fetched (D6, non-goal).
- Header repeats the card (dot, title, daemon, model, usage) and hosts the action buttons owned by [attention-and-interventions.md](attention-and-interventions.md) (Interrupt, Open in web UI) and that agent's pending dialogs inline (answer buttons per attention spec).
- Closes automatically, with a toast notice, when the selected agent is archived or deleted or its daemon entry removed (D27). `agent_timeline_reset` → buffer cleared and refetched, state `refetching` shown as a subtle "reloading" indicator (D27).
- Daemon disconnected: panel keeps content, shows stale banner, no live updates; on reconnect it refetches (live-state rebuild).
- Selection of an agent that vanished in a rebuild: panel closes with notice "agent no longer on <daemon>".

### Usage display (D16)
- Per card: `tokens` = `usage.tokens.total` when present, else `input+output`; shown compact (e.g. 12.3k); `cost` exactly as the daemon reports it, no currency symbol assumed or conversion (D16) — rendered as a plain number with the daemon's precision [TODO(verify): the stats payload carries no currency field; display as bare number, e.g. `0.42`]; context usage percent when `contextUsage.percent` is present.
- `usage == none` → "—" for tokens and cost (no live session, or not yet fetched). A usage older than its refresh window shows a muted "as of <time>" from `takenAt` instead of presenting stale data as live.
- Totals bar: `usage_totals` over the filtered cards, always labelled "N of M agents" (covered of total) — a partial sum is never presented as complete (D16). When `covered == of`, label still reads "M of M agents".
- Polling schedule (executed by the client crate / app binary via effects, D16): running agents every 10 s; once after `turn_completed`/`turn_failed`; idle agents once on connect (after rebuild); closed agents once on connect (will usually be `none`). Polling pauses for a stale daemon.

### Degraded daemons (D26)
Per-daemon banner on the daemon chip/popover for each `DegradedReason` (only `no_observability`: "update daemon for health & logs", stage 2 panels). Cards and dialogs behave normally. Unknown status values render as status "unknown" with the idle dot.

## Data & persistence touchpoints
- Reads `MonitorState.agents`, `drill`, `daemons`; writes only `UiState` (filters, selection, rail state) via commands. No persistence beyond what the config spec defines (D8).
- Selection key is `(serverId, agentId)`; if `serverId` is not yet known (daemon never connected) there are no cards for it.

## Error handling & edge cases
| Condition | Expected behavior |
|-----------|-------------------|
| 0 daemons configured | Empty state pointing to "Add daemon" (`daemon.add`) |
| All daemons unreachable | Cards (if any previous) dimmed with stale labels; chips show connection state (D25) |
| Unknown status enum | "unknown" label, idle dot, rank with idle (D26) |
| Agent removed while selected | Drill closes, notice (D27) |
| Hundreds of cards | Virtualised above ~100 (D28); filtering still O(n) over ids |
| Very long title/tool label | Truncated in model (caps in live-state) and with ellipsis in the view |
| Usage `contextUsage.percent` null | Omit the context indicator |
| Selected agent's daemon becomes duplicate | Drill closes (agents of a duplicate entry are dropped) |
| Filter selects a daemon later removed | Entry ignored; effectively "all" if none remain |

## Dependencies on other specs
- [live-state.md](../architecture/live-state.md) — `AgentEntry`, `ActivitySummary`, `card_order`, `usage_totals`, drill buffer reducers.
- [attention-and-interventions.md](attention-and-interventions.md) — attention flags, rail, interrupt / open-in-web-UI buttons.
- [daemon-connections.md](daemon-connections.md) — connection state shown on chips, stale semantics.
- [design-system.md](../architecture/design-system.md) — tokens, status dot, card, badge, status bar.
- pi-studio `docs/agent-stream-events.md` and status-map/status-dot sources (read-only references).

## Acceptance criteria
- [ ] Given a local daemon at its default address and the dashboard launched for the first time, when connected, then non-archived agents appear as cards within 2 s with no input. (D10, D15; brief 1)
- [ ] Given a connected daemon, when an agent starts a turn and calls a tool, then its card shows the tool label and last output line within 500 ms of the daemon broadcast; usage shows tokens/cost and the totals bar reads "N of M agents". (D6, D16, D33; brief 2)
- [ ] Given a synthetic load of 10 daemons × 50 streaming agents, when idle, then dashboard CPU < 2%, RSS < 300 MB and ≤ 10 view updates/s. (D28, D33; brief 13)
- [ ] Given 500 agents on one daemon, then the view carries ≤ ~200 cards per emit and scrolling shows all of them. (D28)
- [ ] Given agents in states attention/running/error/idle/closed, then cards render in exactly that order and the closed card is dimmed; archiving an agent removes its card. (D15)
- [ ] Given streaming deltas on a running agent, then card positions do not change while only text fields update. (D15, D28)
- [ ] Given status chips and daemon chips, when two chips are selected, then only matching cards remain and the totals/counts update; clearing restores all. (D11, D15)
- [ ] Given statuses `initializing/idle/running/error/closed`, then dots map to queued/idle/running/error/closed (closed muted, never green) and a pending dialog overrides with the warning colour. (D6, D15, D20)
- [ ] Given an agent with no live session, then usage reads "—" and totals read "N of M agents" with N < M. (D16)
- [ ] Given a selected agent, when the drill-down opens, then it shows the fetched timeline then live deltas for assistant text, reasoning and tool calls with status, and replaces the attention rail; deselect restores it. (D6, D15)
- [ ] Given a drilled agent that streams more than 2,000 items, then the panel holds at most 2,000 and shows "earlier output not shown". (D17)
- [ ] Given the selected agent is archived or deleted, then the panel closes with a notice; given `agent_timeline_reset`, then it refetches. (D27)
- [ ] Given a disconnect, then cards dim with "stale since <time>"; after reconnect the cards refresh with no user action. (D25)
- [ ] Given a daemon without `daemonObservability`, then a per-daemon banner shows and cards and dialogs still work. (D26)
- [ ] Given an agent with a pending `confirm`/`select` dialog, then its card shows the attention flag and warning dot, sorts first, and the flag clears when `agent_ui_resolved` arrives. (D15, D36)
- [ ] Given hostile strings (HTML, 1 MB title) in daemon data, then the UI renders them as truncated plain text. (D28)

## TODO(verify)
- Drill-down text rendering scope (plain vs. minimal markdown) — default plain text.
- Cost unit/currency: stats payload `cost: number` has no unit in `packages/protocol/src/messages.ts` (`agentSessionStatsResponseSchema`) — displayed as bare number.
- Whether usage should be polled for `closed` agents (no live session expected; one attempt at connect is proposed).
