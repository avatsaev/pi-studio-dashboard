# Task 004 — Focus highlight and settings menu UI

- **Sprint:** sprints/sprint-008-notifications-tray
- **Status:** backlog
- **Type:** feature
- **Area:** frontend + L6 menus
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-003, sprint-007-attention-interventions/task-005

## Goal
Make the rail expand, scroll to and highlight the focused item, and expose notification/close-to-tray/theme settings as checked View-menu entries.

## Context / why
Click-to-focus (T002) lands in `UiState.focusItem`; the user must see the item. Settings are commands with menu checked states, not frontend-owned state (golden rule 5).

## Scope references
- `swe/specs/features/notifications-and-tray.md` § Click → focus the item, § Commands
- `swe/specs/architecture/app-structure.md` § Command registry (menus), § UiState (`focusItem`)
- `swe/specs/architecture/design-system.md` § Dialog attention item (highlight state)
- `crates/shell/src/menus.rs`, `crates/shell/src/state.rs`
- `apps/pistudiodashboard/src/components/AttentionRail.tsx`, `MenuBar.tsx`
- `apps/pistudiodashboard/src/lib/types.ts`

## What to build
- Menus: View menu gains checked entries for "Notify when input needed" (`settings.setNotification needsInput`), "Notify on failures" (`failures`), "Close to tray" (`settings.setCloseToTray`, disabled with reason when no tray), and a theme group (`view.setTheme dark|light|system`) with the current value checked. Checked/enabled state and disabled reasons come from the engine/shell; the menu bar renders them verbatim. Keep `MENU_ORDER = Daemon, Agent, View`.
- Rail focus: when the view's `ui.focusItem` is set, the rail expands, the item scrolls into view and gets a highlight style; the highlight clears after 3 s or on the next user interaction (the clearing is a UI-only timer; if the contract requires it, dispatch an existing `ui.set`-style command to clear `focusItem`, no new business logic). A summary-notification click expands the rail with no highlight.
- Failure items focused by notification show "View agent".
- Respect reduced motion for the highlight.

## Out of scope
- Notification service, tray, close-to-tray logic — sprint-008/task-001..003.
- Attention rail item rendering — sprint-007/task-005.

## Acceptance criteria
- [ ] Given `focusItem` set to an existing dialog item in a collapsed rail, then the rail expands, scrolls the item into view and highlights it; the highlight disappears after 3 s or on the next interaction.
- [ ] Given the View menu, then the notification toggles, close-to-tray and theme entries show checked states from the engine and invoking one dispatches the matching command with the right args.
- [ ] Given no tray available, then the close-to-tray entry is disabled and shows its reason.
- [ ] Given a summary-notification click, then the rail is expanded with no highlighted item.
- [ ] Given a drill-down open when a notification is clicked, then the rail is visible (engine closed the drill, frontend renders the rail).

## Test / verification plan
- Tests: component tests for highlight/scroll (jsdom `scrollIntoView` mock, fake timers) and menu checked states/dispatch; shell tests for menu model.
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test`.
- Frontend: `cd apps/pistudiodashboard && npm run check && npm run lint && npm test` pass.
- Snapshots: `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` in **both** dark and light themes; open and look at each image (golden rule 5); record what was checked in the task summary.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- Snapshot should include a highlighted rail item and the open View menu in both themes.
