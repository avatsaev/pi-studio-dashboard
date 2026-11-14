# Task 004 — `agent.openInWebUi` and `open_url` hook

- **Sprint:** sprints/sprint-007-attention-interventions
- **Status:** backlog
- **Type:** feature
- **Area:** L5 engine + app binary
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** sprint-005-daemon-connections/task-002

## Goal
Hand off any agent or dialog to the pi-studio web UI in the system browser with a password-free deep link.

## Context / why
Input/editor dialogs and anything not answerable here are handed off (D13, D24). The URL follows pi-studio's deep-link convention (sprint-082): the web UI never auto-connects from a bare link, so `host` and `connect=1` are required.

## Scope references
- `swe/specs/features/attention-and-interventions.md` § `agent.openInWebUi`, § Classification
- `swe/specs/architecture/app-structure.md` § Services hooks (`open_url`)
- `crates/engine/src/` (URL builder + command; `params.rs`)
- `apps/pistudiodashboard/src-tauri/src/services.rs` (real `open_url`)
- `crates/testkit/` (recording `open_url`)
- `crates/config/` entry field `webUiUrl` (sprint-001/task-004)

## What to build
- URL builder (pure, unit-tested): `base = entry.webUiUrl` (local default `http://localhost:4173`; http/https only, else disabled); `host = ws(s)://<entry.host>:<entry.port>` (scheme from `entry.tls`); `url = base_without_query_or_fragment + "/?host=" + percent_encode(host) + "&connect=1" + "#agent=" + percent_encode(agentId)`. Password and pairing fragments are never included.
- Command `agent.openInWebUi {serverId, agentId}` calls `EngineServices::open_url(url)`. Disabled with "no web UI URL configured for <daemon>" when no URL; disabled with "open not available" when the hook is absent. The engine refuses non-http(s) URLs.
- App binary: real `open_url` hook opens the system browser from Rust (no webview permission; golden rule 6) after rejecting non-http(s) schemes. Check the licence of any added crate (e.g. `open`/`opener`) per golden rule 9 and record it in the task summary.
- The `actions[]` of dialog and agent items (computed in T005) reference this command.

## Out of scope
- Rail/button rendering — sprint-007/task-005.
- "View agent" (`agent.select`) — sprint-006/task-005.
- Bracketed IPv6 host handling beyond percent-encoding (TODO(verify)).

## Acceptance criteria
- [ ] Given daemon `127.0.0.1:6767` (no TLS), web UI `http://localhost:4173`, agent id `a b`, then the URL is `http://localhost:4173/?host=ws%3A%2F%2F127.0.0.1%3A6767&connect=1#agent=a%20b`; with `tls:true` the host scheme is `wss`.
- [ ] Given a `webUiUrl` already containing a query or fragment, then they are stripped before the pieces are appended.
- [ ] Given an entry without a web UI URL, then `agent.openInWebUi` is disabled with "no web UI URL configured for <daemon>" and the engine refuses it.
- [ ] Given a `webUiUrl` with scheme `file:` or `javascript:`, then it is rejected and `open_url` is never called.
- [ ] Given a daemon entry with a stored password, then the URL contains no password material.

## Test / verification plan
- Tests: table-driven URL builder unit tests; engine test with the recording `open_url` hook. `cargo test -p pistudiodashboard-engine`.
- Manual: from a running app with a local daemon, run `agent.openInWebUi` via the control socket; the default browser opens the expected URL.
- Full gate: `cargo xtask ci` passes (golden rule 8).

## Notes
- TODO(verify): bracketed IPv6 hosts in the `host` param.
- Record the licence of any new opener crate in the task summary.
