# Task 005 — Daemon chips and dialogs UI — Summary

- **Sprint:** sprints/sprint-005-daemon-connections
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
Frontend (thin; every action is a command id): `DaemonChips` rewritten (one chip per `daemonChips[]`: name + count button that toggles `view.filterDaemons`, connection pill with the spec label/tone, `unencrypted` badge, duplicate / no-observability glyphs that open their explanation, inline **Enter password** on `auth_failed`, a `⋯` menu with Edit…/Set password…/Connect/Disconnect/Diagnostics/Remove…; failure sentences with the spec wording below the row; empty state offers **Detect local daemon**, the grid's empty state keeps **Add daemon**). `DaemonFormDialog` (add/edit, per-field first errors from `bad params: {"errors":[…]}`, plaintext-`ws://` warning, write-only password + remember; add without "remember" stores the password afterwards with `daemon.setPassword remember:false`), `PasswordDialog` (keychain-unavailable note), `RemoveDaemonDialog` (names the daemon, "no undo"), `DaemonDialogs` host, `Field` primitives, `lib/daemonText.ts` (wording), `lib/commands.ts` (`execute`, `setDialog`, `fieldErrors`). `ConnectionPill` now follows the state table (never-live backoff = danger "Unreachable"; was-live backoff = warning "Reconnecting · next try in Ns"). Typed text and passwords stay component-local; no password reaches any view value.
Rust: chip view gained `unencrypted` (plain ws + password + non-loopback host) and `degraded` (`crates/engine/src/daemon_view.rs`, wired in `view.rs`). Menu: `daemon.add` ("Add Daemon…", Cmd+N), `daemon.edit`, `daemon.remove` are now in the Daemon menu; a menu click/shortcut (`menus::invoke`, no params) opens the add form, or the edit/remove dialog for the selected/only daemon (several daemons: "use the menu on a daemon chip"); `engine.execute` with params still reaches the engine commands.

## Files
`apps/pistudiodashboard/src/{App.tsx,test-support.ts,lib/{types,daemonText,commands}.ts}`, `components/{DaemonChips,DaemonDialogs,DaemonFormDialog,PasswordDialog,RemoveDaemonDialog,Field,ConnectionPill,Dialog}.*` (+ tests), `crates/engine/src/{daemon_view.rs,view.rs,daemon_cmds.rs,lib.rs}`, `crates/shell/src/{menus.rs,tests.rs}`.

## Build & test results
- `npm run check` pass; `npm run lint` pass (0 warnings); `npm test` 23 files / 202 tests pass (new: DaemonChips 12, DaemonDialogs 8, plus pill spec).
- `cargo test -p pistudiodashboard-shell -p -engine -p -ingest -p -model` pass (shell 17 incl. menu/dialog tests, engine 43 incl. `unencrypted_*`); `cargo test -p pistudiodashboard` 29 pass; `cargo clippy ... --all-targets` clean; rustfmt clean on my files.

## Acceptance criteria
- [x] every state renders label + dot + tone, text label always present (DaemonChips tests)
- [x] auth_failed "Enter password" opens the password dialog (`ui.set`), submit runs `daemon.setPassword`; the password appears in no view value or `ui.inspect` (dialog test + shell redaction test)
- [x] 403 message names host and `PI_STUDIO_HOSTNAMES`; unreachable shows retry seconds
- [x] unencrypted badge only for tls:false + password + non-loopback (Rust test + component test)
- [x] add dialog shows the command's field errors (first per field) and stays open
- [x] remove confirm runs `daemon.remove` on confirm, nothing on cancel
- [x] duplicate / no-observability glyph renders and opens its explanation
- [x] hostile/long names render as truncated plain text (chip row + remove dialog tests)

## Smoke against pi-studio
Headless app (`PI_STUDIO_LISTEN=127.0.0.1:6790 ./target/debug/pistudiodashboard --headless --control 7879 --config-dir /tmp/pistudio-s5/cfg`), daemons added over the control socket; agents from the pi-studio CLI (`hello`, `#stream fail`, `#ui confirm`, `#todo plan`, `#stream reasoning`). Observed chips: Local + "dev daemon" (same daemon, real duplicate glyph) live with `0.0.115`; `127.0.0.1:6799` -> `backoff`, `unreachable`, "Can't reach 127.0.0.1:6799. Retrying in 18s."; a throwaway production daemon (`PI_STUDIO_PASSWORD=smoke-pw`, home `/tmp/pi-studio-auth`, :6791, killed afterwards) with a wrong password -> `auth_failed`, password dialog raised, 401 sentence (and the keychain note, since this box has no Secret Service); `setPassword smoke-pw` -> chip `live`, password not in `ui.inspect`. A non-loopback `192.0.2.10` entry with a password showed the `unencrypted` badge. Add with 64+ char name returned the `invalid_name` error shown under the field. Not checked: a 403 chip against a real daemon (needs `PI_STUDIO_HOSTNAMES`; wording covered by unit test), a real keychain.
Screenshots (real view JSON from `ui.inspect`, `npm run snapshot -- --view`), `done/screenshots/task-005-006-*-{dark,light}.png`: `chips` (all states incl. Auth failed + Enter password, Unreachable, Connecting, unencrypted, duplicate glyph, truncated hostile name, messages), `chip-menu`, `password-dialog`, `add-form`, `edit-form` (plaintext warning), `remove-dialog`. All read; no overflow or colour-only states found. The static snapshot backend cannot show typed-then-submitted validation errors (covered by component tests).

## Deviations / follow-ups
- Failure sentences are plain tinted rows, not `Banner`, because `Banner` carries `role=status` and collides with the status bar's role in existing tests.
- Menu-only targeting (Cmd+N/Edit/Remove on the selected or only daemon) is a small addition so the spec's Daemon menu works; chips use their own menu with the entry id.
- TODO(verify): the "ws" muted label for plaintext without a password (spec) is not drawn.
