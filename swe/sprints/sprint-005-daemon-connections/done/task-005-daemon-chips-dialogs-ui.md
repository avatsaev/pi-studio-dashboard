# Task 005 — Daemon chips and dialogs UI

- **Sprint:** sprints/sprint-005-daemon-connections
- **Status:** done
- **Type:** feature
- **Area:** apps/pistudiodashboard/src (frontend)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** task-003, sprint-003-design-system/task-002

## Goal
Draw daemon chips with every connection state and error message, plus the daemon form, password and remove-confirm dialogs.

## Context / why
Engineer-facing surface of daemon connections. The frontend is thin (rule 5): it draws the view and sends command ids; no connection logic in TS.

## Scope references
- swe/specs/features/daemon-connections.md § Connection states shown on daemon chips, § Error messages, § "Unencrypted" badge, § Web UI URL per entry, § Degraded mode
- swe/specs/architecture/design-system.md § Components to rebuild (ConnectionPill, Banner, Dialog)
- swe/specs/features/agent-monitor.md § View shape (`daemonChips`)
- swe/specs/architecture/app-structure.md § Webview boundary, § UiState
- `apps/pistudiodashboard/src/{App.tsx,lib/types.ts,test-support.ts}`
- `apps/pistudiodashboard/src/components/` (new `DaemonChips.tsx`, `DaemonFormDialog.tsx`, `PasswordDialog.tsx`, `RemoveDaemonDialog.tsx` + `.module.css` + `.test.tsx`)
- `crates/shell/src/view.rs` (chip view data — only if a field is missing)

## What to build
Frontend (React + TS, CSS Modules, `--pi-*` tokens only — no colour literals):
- **Daemon chip row** (grid header area from the skeleton): one chip per `view.monitor.daemonChips[]` entry: name, state dot + label per the table — `connecting` info "Connecting…"; `connected` success, tooltip `<hostname> · pi-studio <version>` (version omitted if missing); `reconnecting` warning "Reconnecting · next try in Ns"; `unreachable` danger "Unreachable"; `auth_failed` danger "Auth failed" with inline **Enter password** action (opens `UiState.dialog` password via `ui.set`/command); `host_rejected` danger "Host not allowed"; `disconnected` muted "Disconnected". Chip click toggles `view.filterDaemons` membership; a secondary menu per chip offers Edit/Remove/Connect/Disconnect (each is `invoke`/`execute` of the registry command). Extras: duplicate-entry warning glyph ("duplicate entry — same daemon as `<other name>`"), a 'no observability' glyph (when the daemon lacks `daemonObservability`; text 'Health and logs need a newer pi-studio daemon.'; stage 2 panels use it later), the **unencrypted badge** (D32: `tls==false` AND password configured AND host not loopback; tooltip 'Password and agent data travel unencrypted. Use `wss://` if the daemon supports it.'; plain `ws://` without password to non-loopback → muted 'ws' label only). The view supplies the booleans (computed in the shell/engine, not TS).
- **Error messages** as inline text/tooltip with the spec wording: unreachable 'Can't reach `<host>:<port>`. Retrying in `<n>`s.'; 401 'Authentication failed for `<name>`. Check the password.'; 403 '`<host>` is not allowed by the daemon. Add `<host>` to `PI_STUDIO_HOSTNAMES` on the daemon, or connect by IP address.'; handshake invalid '`<host>:<port>` didn't answer like a pi-studio daemon.'; frame too large 'Connection reset: oversized message from daemon.'; keychain unreadable 'Couldn't read the saved password — enter it to connect (it won't be saved).'
- **Daemon form dialog** (`UiState.dialog.kind == "daemonForm"`, mode add/edit): fields name, host, port (default 6767), tls checkbox, web UI URL, password (password input, write-only; empty on edit = unchanged) + 'remember' checkbox; submit → `daemon.add` / `daemon.edit` (+ `daemon.setPassword` when a password was typed on edit); per-field errors returned by the command render under each field (first error per field); inline warning for non-loopback plaintext `ws://`. The typed-but-unsubmitted text stays component-local; the password is never echoed or stored in any view value.
- **Password dialog** (`kind == "password"`): password input + remember checkbox → `daemon.setPassword`; shows the keychain-unavailable note when flagged.
- **Remove confirm** (`kind == "removeDaemon"`): names the daemon and says there is no undo → `daemon.remove`.
- Empty state (no daemons) points to 'Add daemon' (`daemon.add` opens the form) and 'Detect local daemon' (`daemon.detectLocal` from task-004).
- `lib/types.ts` mirrors any new view fields; `test-support.ts` `sampleView()` gets chips in each state.

## Out of scope
- Diagnostics panel (task-006); card grid and filters bar beyond the chips' filter toggle (sprint-006/task-004); attention rail (sprint-007).
- Any connection or validation logic in TS beyond rendering command errors.
- Design tokens/primitives themselves (sprint-003).

## Acceptance criteria
- [ ] Given chips in each connection state, then each renders the label/dot/colour of the state table, never relying on colour alone (text label present).
- [ ] Given a chip in `auth_failed`, when 'Enter password' is clicked, then the password dialog opens, submit calls `daemon.setPassword`, and no password appears in any rendered view value or `ui.inspect`. (D18, D32)
- [ ] Given a 403 chip, then the message names the host and `PI_STUDIO_HOSTNAMES`; given unreachable, then it shows the retry seconds.
- [ ] Given a daemon with `tls:false`, a password and a non-loopback host, then the unencrypted badge shows; given `wss://` or loopback, then no badge. (D32)
- [ ] Given the add dialog with empty host, port 0/65536, blank/duplicate name or a non-http(s) URL, when submitting, then field errors from the command are shown and the dialog stays open. (D10)
- [ ] Given the remove confirm, when confirmed, then `daemon.remove` runs; when cancelled, nothing runs.
- [ ] Given a duplicate or no-observability daemon, then the glyph renders and opens its explanation.
- [ ] Given hostile text (HTML, very long) in a daemon name or hostname, then it renders as truncated plain text.

## Test / verification plan
- Component tests (`*.test.tsx`) for chips (all states + badge + glyphs), form dialog (validation display, password write-only), password dialog, remove dialog.
- Manual/visual: view the chip row and each dialog in dark and light via snapshot.
- `cd apps/pistudiodashboard && npm run check && npm run lint && npm test` pass.
- `cd apps/pistudiodashboard && npm run snapshot -- --out <file>.png` in BOTH themes (dark and light); open and look at each image (golden rule 5) and record what was checked in the task summary.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): where the diagnostics panel sits (task-006 decides); this task only provides the chip's entry point.
- TODO(verify) (spec): final wording belongs to the view layer; the quoted strings are intent from the spec.
- Golden rule 5: add `sampleView()` data for any new state and check `npm run snapshot` output by eye.
