# Task 003 — Auth, handshake errors, RTT, diagnostics

- **Sprint:** sprints/sprint-004-protocol-client
- **Status:** done
- **Type:** feature
- **Area:** crates/client (L4)
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Add bearer-subprotocol auth, 401/403 mapping to terminal states, ping/pong RTT and the diagnostics counters to the client.

## Context / why
The daemon authenticates with a password carried as a `Sec-WebSocket-Protocol` bearer token; a wrong password must stop the retry loop (D25) and a rejected Host must say which host (403). Liveness and counters feed the diagnostics panel (D29).

## Scope references
- swe/specs/architecture/protocol-client.md § Connection state machine, § Ping/pong RTT, § Diagnostics counters, § Error handling & edge cases
- swe/specs/features/daemon-connections.md § Error messages
- `crates/client/src/{conn.rs,counters.rs,rtt.rs}`
- `crates/protocol/src/{liveness.rs,error.rs}` (from task-001)

## What to build
Modify `crates/client`:
- Password: when the entry has a password the upgrade request offers subprotocol `pi-studio.bearer.<pw>`; no password → no subprotocol header. The secret is held in a `Secret` wrapper whose `Debug`/`Display` print `***` and is dropped/zeroised on `remove`. Reject at the API (`connect`/`restart` returns `Err(InvalidPassword)`, no connection) any password that is not a valid HTTP token — until pi-studio's encoding is verified (see Notes) do NOT percent-encode by guess.
- Upgrade-response mapping: `101` → handshaking; HTTP `401` → `auth_failed` (ConnError `auth_failed`, **no retry**, state terminal until `connect`/`restart`); HTTP `403` → `host_rejected` (ConnError carries the host used, so the UI can name `PI_STUDIO_HOSTNAMES`; **no retry**); any other status/transport error → `backoff`. `connect` from `auth_failed`/`host_rejected` goes to `probing`.
- Handshake failures: socket closed right after `hello` → backoff; after 3 consecutive handshake failures `lastError.kind = handshake_rejected` and the delay stays at the 30 s cap.
- Inbound frame rules: text frame > 16 MiB → close, `framesTooLarge` +1, `ConnError{frame_too_large}`, backoff; non-JSON/non-object/no string `type` → drop, `malformed` +1, connection stays live; binary frames ignored, `binaryFramesIgnored` +1; unknown top-level/session `type` → `unknownTypes[type]` +1 (bounded ≤ 64 distinct keys, then bucket `other`); unknown enum values → `unknownEnumValues` +1.
- Ping/pong: while `live`, every 15 s send top-level `Ping{requestId, clientSentAt}`; on `pong` compute `rtt = local_now − clientSentAt` using the local clock only; keep last + EWMA; emit `Rtt{ms}`. Two consecutive unanswered pings (30 s) → treat as dead: close, backoff.
- Diagnostics counters per connection (names fixed by the spec): `state, lastError{kind,message,at}, reconnectCount, messagesTotal, messagesPerSecond (1 s window), unknownTypes, unknownEnumValues, malformed, binaryFramesIgnored, framesTooLarge, droppedEvents, lateResponses, rttMs{last,ewma}, connectedSince?`. Emit as `Diagnostics` event at most 1/s, only while a counter changed. `lateResponses`/`droppedEvents` exist but are driven by later tasks (004, backpressure).
- Never log or put in any event: the password, bearer string, subprotocol value, or dialog payload text; URLs are logged without credentials.

## Out of scope
- Request correlation, `OutboundRequest` (task-004); sync (task-005).
- The password prompt, keychain, `daemon.setPassword` (sprint-005/task-003).
- UI messages for the errors (sprint-005/task-005).
- Foldable-event dropping (`droppedEvents` is only declared here; backpressure behaviour is exercised by the sprint-009 load harness and sprint-006 stream coalescing).

## Acceptance criteria
- [ ] Given a fake daemon that accepts the upgrade only with subprotocol `pi-studio.bearer.s3cret`, when connecting with that password, then the request offered exactly that subprotocol and the first frame sent is `hello`; with no password no `Sec-WebSocket-Protocol` is offered. (D23)
- [ ] Given upgrade responses 401 and 403 (separately), when connecting, then state ends in `auth_failed` / `host_rejected` respectively, the 403 error carries the host, and no further attempt occurs over a long simulated interval without a user `connect`. (D25)
- [ ] Given `connect` after `auth_failed` with a new password, then a new epoch starts at `probing`.
- [ ] Given a 17 MiB text frame, then the connection closes, `framesTooLarge == 1`, `lastError.kind == frame_too_large` and the client reconnects with backoff. (D28)
- [ ] Given the frame `{not json`, then `malformed == 1` and the state stays `live`; given a binary frame, then `binaryFramesIgnored == 1`; given an unknown `type`, then `unknownTypes` counts it and nothing else happens. (D26, D28)
- [ ] Given a daemon that never answers pings, then after 2 missed pongs (30 s simulated) the client closes and backs off; given a pong, then `Rtt` is emitted with `last` and `ewma`.
- [ ] Given captured events and log output during connect, auth failure and reconnect with a password configured, then neither contains the password or `pi-studio.bearer.`. (D29)
- [ ] Given a password containing characters that are not valid HTTP token characters, then `connect` returns `InvalidPassword` and nothing is sent.

## Test / verification plan
- `cargo test -p pistudiodashboard-client` — extend the fake daemon (status-code scripting, oversize/malformed/binary frames, silent pong); add a log-capture test asserting no secret in output (use a `tracing`/`log` capture or the crate's own sink).
- Licence check: any crate added in this task (e.g. a token validator or `tracing`) recorded in the task summary.
- `cargo xtask ci` passes (fmt, web, clippy, layers, test, panic_hunt, doctest, wasm).

## Notes
- TODO(verify) (spec): how pi-studio's own client encodes passwords containing non-token characters (`packages/client`). Until verified, reject such passwords rather than encode; this decides sprint-005 `invalid_password` rules.
- TODO(verify) (spec): daemon behaviour on an unsupported `protocolVersion`.
- System sleep/wake: only missed pongs trigger reconnect; no attempt-counter reset on wake (kept simple, per spec).
