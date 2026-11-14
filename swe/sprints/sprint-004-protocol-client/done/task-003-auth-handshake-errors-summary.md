# Task 003 — Auth, handshake errors, RTT, diagnostics — Summary

- **Sprint:** sprints/sprint-004-protocol-client
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
Already delivered in task-002 (not redone): bearer subprotocol offering, 401 → `auth_failed` / 403 → `host_rejected` terminal states (no retry), `connect` from a terminal state → new epoch at `probing`, 16 MiB frame close, redacted `Secret`.

Added here (crates/client only):
- `Secret`: `Display` = `***`, bytes overwritten on drop; `Secret::validate` (non-empty RFC 9110 token chars). `ClientHandle::connect`/`restart` now return `Result<(), InvalidPassword>`; nothing is sent or started on error; no percent-encoding.
- 403 error text names the host and `PI_STUDIO_HOSTNAMES`.
- `ConnErrorKind::HandshakeRejected` (`handshake_rejected`): after 3 consecutive handshake failures the error code changes and the delay is pinned at the 30 s cap (`backoff::CAP_ATTEMPT`).
- `counters.rs`: all spec counters (`reconnectCount, messagesTotal, messagesPerSecond (1 s window), unknownTypes (≤64 keys then "other", keys capped 64 B), unknownEnumValues, malformed, binaryFramesIgnored, framesTooLarge, droppedEvents, lateResponses, rttMs{last,ewma α=0.2}, connectedSince`) in model `CountersSnapshot`; counters live as long as the entry task (survive reconnects). `Diagnostics` is emitted ≤1/s and only when something changed (also during backoff).
- `rtt.rs`: ping every 15 s while live (`rtt-<n>` ids, local monotonic clock for RTT), `Rtt{ms}` event per matching pong, two consecutive unanswered pings → close + backoff (`timeout`).
- Live loop classifies frames: non-JSON / non-object / no `type` / malformed pong or rpc_error → `malformed`; binary → `binaryFramesIgnored`; unknown top-level type or session type outside `KNOWN_SESSION_TYPES` (the consumed-messages table) → `unknownTypes`; 17 MiB (tungstenite capacity error) → `framesTooLarge` + `frame_too_large` backoff.
- Connection-transition logging via `log` (`daemon entry <id> -> <state> (<error code>)`), no URLs, passwords, frames.

## Files created / changed
`crates/client/src/{counters,rtt}.rs` (new); `{conn,handle,secret,state,backoff,emitter,lib}.rs` (modified); `tests/auth_counters.rs`, `tests/secrets.rs` (new); `tests/{connection,live_daemon}.rs`, `tests/support/fake_daemon.rs` (extended: required subprotocol, pong on/off, pushed frames, close-after-hello). No new crate dependencies; no files outside `crates/client` and this task file.

## Build & test results
```
$ cargo test -p pistudiodashboard-client
19 unit + 9 (auth_counters) + 14 (connection) + 1 (secrets) passed, 3 ignored (live), 0 failed
$ cargo clippy -p pistudiodashboard-client --all-targets -- -D warnings   # clean
$ cargo fmt -p pistudiodashboard-client --check                           # clean
$ cargo xtask layers -> layering: ok (11 crates)
```

## Acceptance criteria
- [x] Subprotocol offered exactly `pi-studio.bearer.s3cret`, first frame `hello`; no password → no header (bearer_subprotocol_is_offered_only_with_a_password)
- [x] 401/403 terminal, 403 carries host, silent for 600 s simulated (wrong_password_…, host_rejected_names_the_host_…)
- [x] `connect` after `auth_failed` with new password → new epoch at probing (wrong_password_is_terminal_and_a_new_password_starts_a_new_epoch)
- [x] 17 MiB frame → `framesTooLarge==1`, `frame_too_large`, backoff, next epoch (oversize_frame_counts_and_reconnects_with_backoff)
- [x] `{not json` → malformed 1 & live; binary → 1; unknown type counted (malformed_binary_and_unknown_frames_…)
- [x] Silent pongs → close after 2 misses (30–46 s simulated) + backoff; pong → `Rtt` + `rttMs{last,ewma}` (two_unanswered_pings_…, pongs_produce_rtt_events_…)
- [x] No `pi-studio.bearer.`/password in events or logs across connect, auth failure, reconnect (secrets.rs, logs captured at DEBUG)
- [x] Invalid token chars → `InvalidPassword`, nothing sent (invalid_password_is_rejected_before_anything_is_sent)
- [x] Extra: 3 consecutive handshake failures → `handshake_rejected` at the 30 s cap (three_handshake_failures_…)

## Smoke against pi-studio
Dev daemon 127.0.0.1:6790 (`live_daemon_rtt_and_diagnostics`, 19 s): Probing → Health(true) → Connecting → Handshaking → Hello → Syncing → Live; first `Diagnostics` at seq 8 (`connectedSince` set, rest 0); after the 15 s ping `Rtt{ms:0}` (loopback, ms resolution) and `Diagnostics{messagesTotal:1, rttLast:0, rttEwma:0}`.
Throwaway production daemon (`node packages/server/dist/daemon/main.js`, `PI_STUDIO_PASSWORD=smoke-pw`, home `/tmp/pi-studio-auth`, `127.0.0.1:6791`; started as a plain background process, killed and cleaned up afterwards; it ran without real pi credentials):
- no password → `AuthFailed`, "daemon rejected the password (HTTP 401)", no retry within 19 s
- wrong password → same `AuthFailed`
- password `smoke-pw` → Live, `Hello{version 0.0.115, daemonObservability:true}`, `Rtt{ms:1}`, `Diagnostics{messagesTotal:1, rttLast:1, rttEwma:1}`
Not smoked: 403 (needs a Host the daemon rejects), 17 MiB/malformed frames against the real daemon (it does not send them).

## Findings / follow-ups
- **tungstenite logs the full upgrade request, including `Sec-WebSocket-Protocol: pi-studio.bearer.<pw>`, at TRACE** (seen in the capture). Our crate never logs it, but the app logger (sprint-005) MUST NOT enable TRACE for `tungstenite`/`tokio_tungstenite`; the secrets test therefore captures at DEBUG. Record in sprint-005 logger setup.
- `Diagnostics` has no `state`/`lastError` fields (model `CountersSnapshot`): those travel in `ConnState` events.
- `unknownEnumValues`, `droppedEvents`, `lateResponses` are declared and wired into the snapshot but driven by later tasks (typed decoders, backpressure, task-004).
- `messagesPerSecond` reports the last completed 1 s window (0 until a second elapses).
- Still TODO(verify): non-token password encoding in pi-studio's own client; unsupported `protocolVersion`.
