# Task 005 — Attention rail UI

- **Sprint:** sprints/sprint-007-attention-interventions
- **Status:** backlog
- **Type:** feature
- **Area:** frontend + L6 view
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-003, task-004, sprint-006-agent-monitor/task-004

## Goal
Render the right-hand attention rail (dialogs and failures), the interrupt confirmation and toasts from engine-computed `actions[]`.

## Context / why
The rail is where the engineer answers dialogs and hands off the rest. The frontend draws `actions` verbatim and holds no rule logic (golden rule 5); enabled/disabled/reason come from the engine. Dialog text is rendered as plain text only.

## Scope references
- `swe/specs/features/attention-and-interventions.md` § Attention item, § Classification (answerable vs hand-off), § `agent.interrupt` flow, § Toasts
- `swe/specs/architecture/design-system.md` § Dialog attention item (and Banner/Toast/Dialog primitives)
- `swe/specs/architecture/app-structure.md` § Webview boundary, § UiState, § Command registry (`view.toggleAttentionRail`)
- `crates/engine/src/query_cmds.rs` / view builder (add `attention: AttentionItem[]` to the monitor view)
- `apps/pistudiodashboard/src/lib/types.ts`
- `apps/pistudiodashboard/src/components/` (new `AttentionRail.tsx` + `.module.css` + test; `ConfirmInterruptDialog.tsx`; card attention flag + Interrupt button in the card component)
- `apps/pistudiodashboard/src/test-support.ts` (`sampleView()` additions)

## What to build
**View (Rust)**: derive `AttentionItem[]` per spec shape `{itemId ("ui:<id>"|"fail:<agentId>"), kind, serverId, daemonName, agentId, agentTitle, raisedAt, stale, dialog?{method, kind, title?, message?, options?, answerability, inFlight, expiresAt?, expired, handOffReason?}, failure?{message?, source}, actions[{id,label,enabled,reason?,command,args}]}`; order newest first by `raisedAt`, stale last, one failure item per agent. Actions: Yes/No for confirm; one button per option in payload order for select; "Open in web UI" (`agent.openInWebUi`) primary for hand-off and secondary for answerable; "View agent" (`agent.select`) on failure items; "Interrupt" on running agent cards (`agent.interrupt confirm:false`). Buttons disabled while `inFlight` or when the engine says so (with `reason`). The rail ignores card filters.

**Frontend**
- `AttentionRail`: header with count; collapsed state shows a count badge; toggle calls `view.toggleAttentionRail`; empty state; items show agent title, daemon name, method, dialog `title`/`message`/options as plain text; countdown "expires in mm:ss" from the local clock → "expired"; failure items; stale styling.
- Card attention flags (pending dialog count, errored) from `attention` fields; Interrupt button on running cards.
- `ConfirmInterruptDialog` (modal, from `ui.dialog == confirmInterrupt`): "Interrupt <agent title> on <daemon>? The running turn is stopped. This cannot be undone. Open questions from this agent stay pending." with [Interrupt (destructive)] [Cancel]; Enter confirms only when the destructive button is focused; submit calls `agent.interrupt {confirm:true}`.
- Toast viewport exists (sprint-003); verify engine toasts from T001–T003 display.
- No business logic: no option validation, no expiry rules beyond the countdown display.

## Out of scope
- Focus/highlight of an item from a notification click (`focusItem`) and View-menu checked states — sprint-008/task-004.
- The drill-down panel (rail is replaced while it is open, D15) — sprint-006/task-006.
- Health/logs UI — sprint-010.

## Acceptance criteria
- [ ] Given a pending confirm, then the rail shows agent title, daemon, method, plain-text title/message and Yes/No buttons; clicking Yes invokes `dialog.respond {confirmed:true}` and buttons render disabled while `inFlight`.
- [ ] Given a select with options [a,b], then two buttons in that order are shown; clicking `b` invokes `dialog.respond {value:"b"}`.
- [ ] Given an editor/input dialog or select without options, then no answer controls are shown and "Open in web UI" is the primary action (disabled with its reason when the engine says so).
- [ ] Given a dialog with `expiresAt`, then a countdown shows and flips to "expired" with buttons disabled; given a stale item, then it sinks to the bottom with disabled controls and its reason.
- [ ] Given a message containing `<script>` or markup, then it is rendered as literal text.
- [ ] Given a failure item, then it shows the message and a "View agent" action, and disappears when the view no longer contains it.
- [ ] Given the Interrupt button on a running card, when clicked, then the confirm dialog shows the required copy; Cancel sends nothing; confirming calls `agent.interrupt {confirm:true}`.
- [ ] Given the rail collapsed, then a count badge is visible; given a card filter hiding the agent, then its dialog stays in the rail.
- [ ] Given a dialog broadcast by the daemon, then it is visible in the rail within 500 ms of ingest (view coalescing ≤ 100 ms, D28/D33), checked by a timing assertion in the integration/ingest test.

## Test / verification plan
- Tests: component tests for rail and confirm dialog (command + args per button, disabled/reason rendering, plain-text escaping, countdown with fake timers); Rust view-builder tests for order, merge and actions.
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test`.
- Frontend: `cd apps/pistudiodashboard && npm run check && npm run lint && npm test` pass.
- Snapshots: `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` in **both** dark and light themes; open and look at each image (golden rule 5); record what was checked in the task summary.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- TODO(verify): whether `optionDetails` descriptions should be rendered (not in v1).
- `sampleView()` must cover confirm, select, editor, failure, stale and expired items so snapshots exercise every state.
