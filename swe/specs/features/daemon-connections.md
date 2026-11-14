# Feature — Daemon Connections

> Part of: [overview.md](../overview.md)
> Dependencies: [../architecture/protocol-client.md](../architecture/protocol-client.md), [../architecture/config-and-secrets.md](../architecture/config-and-secrets.md), [../architecture/live-state.md](../architecture/live-state.md), [../architecture/app-structure.md](../architecture/app-structure.md), [agent-monitor.md](agent-monitor.md), [attention-and-interventions.md](attention-and-interventions.md)
> Source brief: [daemon-monitoring-dashboard](../../grills/daemon-monitoring-dashboard.md) — D3, D10, D18, D19, D23, D24, D25, D26, D29, D32, D35, D37

## Purpose

How the engineer gets daemons into the dashboard and sees their connection health: auto-detecting the local daemon, adding/editing/removing remote ones (direct `ws://`/`wss://`, optional password, no relay — D3), the per-daemon connection state and error messages, identity (`serverId`) handling, degraded-mode banners, the "unencrypted" badge, per-daemon web UI URL and the diagnostics panel. Wire/state-machine mechanics live in [protocol-client.md](../architecture/protocol-client.md); persistence and keychain in [config-and-secrets.md](../architecture/config-and-secrets.md).

## Public contract

### Daemon entry (persisted shape owned by config spec; fields restated here for UX)

| Field | Type | Rules (decisions of this spec unless D-cited) |
|-------|------|-----------------------------------------------|
| `entryId` | string (UUID v4) | Generated on add; never edited; keys the keychain account (D18, D19). |
| `name` | string | Required, trimmed, 1–64 chars, unique case-insensitively among entries (else "name already used"). |
| `host` | string | Required, non-empty (D10). Hostname, IPv4, or IPv6 literal (stored without brackets). No scheme, no port, no path, no whitespace; ≤ 253 chars. |
| `port` | u16 | 1–65535 (D10). Default 6767 in the dialog. |
| `tls` | bool | `false` = `ws://`/`http://`, `true` = `wss://`/`https://` (D10, D32). |
| `password` | write-only | Optional. Never part of the entry, view, or `UiState` (D18, D32). Dialog field is a password input; empty on edit means "unchanged". |
| `webUiUrl` | string? | Optional http(s) URL; local auto-detected entry defaults to `http://localhost:4173` (D24). |
| `lastSeenServerId` | string? | Set by the app after each handshake (D19). |
| `autoDetected` | bool | True for the entry created by local detection (D10). |

### Commands (camelCase `<area>.<verb>`, registry commands; frontend/CLI/agents use the same entry points)

| Command id | Args | Effect | Errors |
|------------|------|--------|--------|
| `daemon.add` | `{name, host, port, tls, webUiUrl?, password?}` | Validates, creates entry (new `entryId`), stores password via `Services` keychain hook if given, then starts connecting. | `invalid_name`, `duplicate_name`, `invalid_host`, `invalid_port`, `invalid_url`, `too_many_daemons` (> 50, see config spec), `keychain_unavailable` (non-fatal: password kept in memory only, warning returned; D18). |
| `daemon.edit` | `{entryId, name?, host?, port?, tls?, webUiUrl?}` | Updates entry; if host/port/tls changed, reconnects and treats the next `serverId` per D19. Password changes go through `daemon.setPassword`. | as above + `unknown_entry` |
| `daemon.remove` | `{entryId}` | Disconnects, removes the entry, deletes its keychain password (D18), clears that daemon's cards/attention items from state (unless another entry resolves to the same `serverId`, then only the duplicate warning clears). Requires confirmation in the UI (no undo). | `unknown_entry` |
| `daemon.connect` | `{entryId}` | Starts/restarts the connection (also the manual retry from `auth_failed`/`host_rejected`/`unreachable`). | `unknown_entry` |
| `daemon.disconnect` | `{entryId}` | Closes and stays idle until `daemon.connect`; cards show as disconnected/stale like an unreachable daemon. Not persisted across restarts. | `unknown_entry` |
| `daemon.setPassword` | `{entryId, password, remember: bool}` | `remember:true` → write to keychain (D18); `false` or keychain unavailable → memory only for this process. Empty password with `remember:true` deletes the stored one. Then reconnect. | `keychain_unavailable` (falls back to memory, returned as warning), `invalid_password` (see TODO) |
| `daemon.showDiagnostics` | `{entryId}` | Opens the per-daemon diagnostics panel in `UiState` (D29). | `unknown_entry` |

`daemon.*` mutate local config only; none sends anything to a daemon except by (re)connecting. All are non-undoable (the app has no undo; live data is not an edit).

### Connection states shown on daemon chips

Chip = the daemon filter chip in the grid header (D11, D15). State comes from the L4 state machine (protocol-client):

| Chip state | From machine state | Visual / label (colour from `--pi-*` status tokens; see design-system) | Cards |
|------------|--------------------|-------------------------------------------------------------------------|-------|
| `connecting` | probing, connecting, handshaking, syncing | info dot, "Connecting…" | unchanged (empty on first connect) |
| `connected` | live | success dot, name; tooltip shows `<hostname> · pi-studio <server_info.version>` | normal |
| `reconnecting` | backoff after having been live | warning dot, "Reconnecting · next try in Ns" | dimmed, "stale since <local time>" (D25) |
| `unreachable` | backoff, never live since (re)start | danger dot, "Unreachable" | dimmed (if any known) |
| `auth_failed` | auth_failed | danger dot, "Auth failed" + inline "Enter password" action | dimmed |
| `host_rejected` | host_rejected | danger dot, "Host not allowed" | dimmed |
| `disconnected` | idle (user) | muted dot, "Disconnected" | dimmed |

Chip extras: a "duplicate entry" warning glyph (D19), a "no observability" glyph when the daemon lacks the stage 2 `daemonObservability` flag (D26), the "unencrypted" badge (D32); each opens its explanation.

Daemon version: `server_info.version` is the daemon's real version (e.g. `"0.0.115"`; it is no longer hard-coded `"1.0.0"`) and is shown in the connected chip's tooltip and in the diagnostics panel header, next to the `serverId` and `hostname`. A missing `version` simply omits it.

### Error messages (verbatim intent; final wording in the view layer)

| Failure | Message |
|---------|---------|
| Unreachable (refused/timeout/DNS) | "Can't reach `<host>:<port>`. Retrying in `<n>`s." plus detail from diagnostics. (D25) |
| HTTP 401 | "Authentication failed for `<name>`. Check the password." with a password prompt (`daemon.setPassword`); no retry loop. (D25) |
| HTTP 403 | "`<host>` is not allowed by the daemon. Add `<host>` to `PI_STUDIO_HOSTNAMES` on the daemon, or connect by IP address." (D25) |
| Handshake invalid / not a pi-studio daemon | "`<host>:<port>` didn't answer like a pi-studio daemon." |
| Frame too large | "Connection reset: oversized message from daemon." (D28) |
| Keychain locked/denied/unavailable | "Couldn't read the saved password — enter it to connect (it won't be saved)." (D29, D18) |

### Local daemon auto-detect (D10, D29)

```
function detectLocalDaemon():
    home = env PI_STUDIO_HOME if set and non-empty else <user home>/.pi-studio
    listen = "127.0.0.1:6767"                       # default; same as daemon-config.ts default
    try read home/config.json (cap 1 MiB), parse JSON
        if env PI_STUDIO_LISTEN is set (non-empty): listen = it          # env wins over the file in pi-studio (overlayEnv)
        else if json.daemon.listen is a string: listen = it
    on any failure (missing, unreadable, invalid JSON, wrong type): listen = default (D29), no error shown
    (host, port) = parse "host:port"  (also "[::1]:port"); on parse failure → default
    if host in {"0.0.0.0", "::", ""}: host = "127.0.0.1"      # wildcard bind → connect via loopback
    return DaemonEntry{name:"Local", host, port, tls:false, webUiUrl:"http://localhost:4173", autoDetected:true}
```

Runs in the app binary (file I/O, env), never in the webview. Runs at startup: if no entry has `autoDetected:true` **and** no entry's `host:port` already equals the detected address, the entry is created and persisted; if one already exists, it is NOT silently rewritten (the user may have edited it) — instead, when detection returns a different address than the stored auto-detected entry and the entry was never edited, it is updated; otherwise left alone. [Rule: an `autoDetected` entry that the user edited sets `autoDetected:false`.] The user can remove the auto-detected entry; it is not re-created on later starts (a `localDetectionDismissed` flag is stored in config, decision of this spec).

### Identity: `serverId` (D19)

Resolved after each handshake. Rules:

| Situation | Behavior |
|-----------|----------|
| First connect of an entry | `lastSeenServerId := serverId`. |
| Same entry reconnects, same `serverId` | Silent rebuild (D25). |
| Entry reconnects with a **different** `serverId` (reinstall, address reused) | Treated as a new daemon: the old daemon's cards, pending items and events are cleared, `lastSeenServerId` updated, a one-time toast "`<name>` is a different daemon now (new id)". (D19) |
| Two entries yield the same `serverId` | One daemon in state; the second entry is marked duplicate: chip warning "duplicate entry — same daemon as `<other name>`"; the second entry's connection is **closed** (so the daemon sees one dashboard client and events are not double-counted) but stays in config; resolving the duplicate (remove/edit one) reconnects it. The entry that connected first is primary. (D19) |
| Primary entry removed while a duplicate exists | The duplicate connects and becomes primary. |

### Degraded mode (D26, D36)

With D36 there are no stage 1 capability flags: every daemon in the field supports everything stage 1 needs (late-join of dialogs uses the existing `agent_ui_list_request`). Degraded mode therefore covers only:

| Reason | Where shown | Text intent |
|--------|-------------|-------------|
| `no_observability` (daemon lacks `daemonObservability`, stage 2) | Health and logs panels of that daemon, and the diagnostics panel — not a grid banner | "Health and logs need a newer pi-studio daemon." |
| Unknown message types / enum values | Diagnostics counters only (never a banner) | Counted and ignored, never fatal (D26) |

Everything else keeps working. (`DegradedReason` in protocol-client is `{no_observability}`.)

### "Unencrypted" badge (D32)

Shown on the chip and in the entry list when `tls == false` **and** a password is configured (stored or in memory) **and** `host` is not loopback (`127.0.0.0/8`, `::1`, `localhost`). Tooltip: "Password and agent data travel unencrypted. Use `wss://` if the daemon supports it." Plain `ws://` without password to non-loopback: no badge (nothing secret sent) but data is still plaintext — shown as a muted "ws" label only. Adding such an entry is allowed (D32); the dialog shows an inline warning.

### Web UI URL per entry (D24)

Field `webUiUrl` is editable; validated `http(s)://…` ≤ 2,048 chars, no credentials in the URL. Used by `agent.openInWebUi` (owned by attention spec): final URL = `<webUiUrl>/?host=<ws(s)://host:port>&connect=1#agent=<urlencoded agentId>` (pi-studio deep link, shipped in sprint-082; no password in the URL). The URL is opened in the system browser by the app binary. When absent: the action is disabled with "Set the web UI URL for `<name>`". The local default `http://localhost:4173` is only a default, not verified reachable.

### Diagnostics panel (`daemon.showDiagnostics`, D29)

One panel per daemon, in `UiState` (open dialog/panel; replaces the rail like the drill-down? — placement left to the shell spec, see TODO). Contents (read-only view of L4 `Diagnostics` events): connection state + since, last error (kind, message, time), reconnect count, connected since, messages/s, total messages, unknown-type counts (top 10 by count), unknown enum count, malformed count, oversized-frame count, dropped events, ping/pong RTT (last + average), server info (serverId, hostname, version, features known/missing), degraded reasons, health probe result. No password, no bearer, no `action` payloads (D29). Updates ≤ 1/s.

## Behavior & algorithms

```
on app start:
    cfg = config.load()                       # may be empty, with warning (D29)
    detected = detectLocalDaemon()            # see above, creates auto entry if needed
    for entry in cfg.daemons (enabled):
        password = keychain.get(entry.entryId) # may fail → state keychain_unavailable → prompt (D29)
        if password unavailable and daemon required one:   # only known after a 401
            connect without it → 401 → auth_failed → prompt
        client.start(entry, password)
on DaemonEvent Hello(entry, serverId):
    resolveIdentity(entry, serverId)          # per table above
on add dialog submit:  validate fields client-side AND in command; show first error per field
```

Password flow with an unreadable keychain: the app first tries to connect with no password (many daemons have none); a 401 raises the prompt; the entered password is remembered in memory (and in the keychain when `remember` and available).

Validation order (dialog and `daemon.add/edit`): name, host, port, URL; all errors returned together as a list keyed by field. Host normalisation: strip surrounding whitespace, lowercase, strip brackets of IPv6; reject any of `/ ? # @ :` (except IPv6) or spaces.

## Data & persistence touchpoints

- Persisted: daemon entries and `localDetectionDismissed` in the config file (see config-and-secrets). Password only in the OS keychain (D18).
- In memory: connection state, `server_info`, degraded set, duplicate marks, in-memory passwords (zeroised on removal/exit), diagnostics counters.
- Reads (not writes) of the local pi-studio `config.json` and environment variables `PI_STUDIO_HOME`, `PI_STUDIO_LISTEN`. Never writes pi-studio's files.

## Error handling & edge cases

| Condition | Expected behavior |
|-----------|-------------------|
| Local `config.json` missing/unreadable/invalid (D29) | Default `127.0.0.1:6767`; no dialog. |
| Detected address changes between runs (user reconfigured daemon) | Untouched auto entry updated to the new address; edited/dismissed entries left alone. |
| Daemon listens on `0.0.0.0` (explicit user config; pi-studio's own default is now `127.0.0.1:6767`, sprint-087, which resolves D35) | Connect to `127.0.0.1`; the wildcard rewrite stays for explicit wildcard configs. |
| Same daemon added as `localhost` and as LAN IP | Same `serverId` → duplicate warning; second connection closed (D19). |
| Entry points at a non-pi-studio server | `bad_handshake` message; keeps backing off at the cap. |
| Daemon restarted (same `serverId`) | Silent rebuild; cards dim "stale since" during the gap, undim after Snapshot (D25). |
| Daemon reinstalled | New `serverId` → old cards cleared + toast (D19). |
| 50-entry limit reached | `too_many_daemons`. |
| Keychain write fails on add | Entry still added, in-memory password, warning "password not saved" (D18, D29). |
| Entry removal while connecting | Task cancelled first, then deletion; no event for a removed entry is applied afterwards. |
| User clicks connect repeatedly | Idempotent: restart only if not already connecting/live. |
| Removing the only remaining entry | App shows the empty state with "Add daemon" (and "Detect local daemon" which re-runs detection and clears `localDetectionDismissed`). |
| Password with disallowed characters | Rejected with a clear message (see TODO). |
| IPv6 / hostnames resolving to multiple addresses | System resolver order; first success wins; no happy-eyeballs in v1. |

## Dependencies on other specs

- [protocol-client.md](../architecture/protocol-client.md) — state machine, 401/403 semantics, degraded set, diagnostics counters.
- [config-and-secrets.md](../architecture/config-and-secrets.md) — entry schema, keychain, `.bak`.
- [live-state.md](../architecture/live-state.md) — clearing a daemon's state on new `serverId`/remove, stale-since dimming.
- [app-structure.md](../architecture/app-structure.md) — `Services` hooks (keychain, local detection, URL opening), dialogs in `UiState`.
- [agent-monitor.md](agent-monitor.md) — dimmed/stale cards; [attention-and-interventions.md](attention-and-interventions.md) — `agent.openInWebUi`.

## Acceptance criteria

- [ ] Given a pi-studio daemon at the default address and an empty config, when the dashboard launches, then an auto-detected entry "Local" (`127.0.0.1:6767`, ws) exists and non-archived agents appear as cards within 2 s with no user input. (D10, D15; brief acceptance 1)
- [ ] Given `$PI_STUDIO_HOME/config.json` with `daemon.listen:"0.0.0.0:7000"` and no `PI_STUDIO_LISTEN`, then detection yields `127.0.0.1:7000`; given `PI_STUDIO_LISTEN=127.0.0.1:7100`, then `7100`; given an unreadable file, then `127.0.0.1:6767`. (D10, D29)
- [ ] Given a wrong password, then the chip shows "Auth failed", a password prompt appears, and no further connection attempt occurs until the user submits; given a 403, the message names `PI_STUDIO_HOSTNAMES` and the host. (D25; brief acceptance 9)
- [ ] Given a connected daemon that is killed, then its cards dim with "stale since <time>" and the chip reads "Reconnecting"; when it restarts with the same `serverId`, state rebuilds with no user action and no notice. (D25; brief acceptance 10)
- [ ] Given a daemon whose `server_info.features` lacks `daemonObservability`, then its health/logs panels show the "needs a newer daemon" state, the grid shows no banner, and everything else works; given an unknown message type, then it is counted in diagnostics and nothing else happens. (D26, D36; brief acceptance 11)
- [ ] Given two entries that resolve to one `serverId`, then exactly one daemon's cards appear, the second chip shows "duplicate entry", and the daemon sees one client. (D19)
- [ ] Given an entry whose address now answers with a new `serverId`, then the old cards are cleared, a toast is shown and `lastSeenServerId` is updated. (D19)
- [ ] Given the add dialog, when host is empty, port is 0 or 65536, name is blank or duplicated, or the web UI URL is not http(s), then submission is rejected with a per-field reason and nothing is persisted. (D10)
- [ ] Given a `ws://` entry with a password to a non-loopback host, then the "unencrypted" badge shows; given `wss://` or loopback, no badge. (D32)
- [ ] Given `daemon.remove`, then the connection closes, the entry disappears from the config file, the keychain item `daemon:<entryId>` is deleted and its cards vanish. (D18)
- [ ] Given a locked keychain at startup and a password-protected daemon, then a prompt appears after the 401, the password works for the session, and nothing is written to disk. (D18, D29)
- [ ] Given `daemon.showDiagnostics`, then the panel shows last error, reconnect count, messages/s, unknown and malformed counts and RTT, and contains no password or `action` content. (D29)
- [ ] Given an entry with `webUiUrl`, when `agent.openInWebUi` runs, then the system browser opens `<webUiUrl>#agent=<urlencoded id>`; given no URL, the action is disabled with an explanation. (D24)
- [ ] Given the connection to a daemon is handshaking, then `hello` carries `clientType:"cli"` and `appVersion:"pistudiodashboard/<version>"`. (D23)

## TODO(verify)

- Password character restrictions for the bearer subprotocol and pi-studio's own encoding (see protocol-client TODO) — decides `invalid_password` rules.
- Whether the web UI default port is `4173` for the production-served UI or only the Vite preview port (D24 gives the value; the deep-link spec in pi-studio confirms the URL form).
- Where the diagnostics panel sits in the layout (replace rail vs. dialog) — owned by [app-structure.md](../architecture/app-structure.md); this spec requires only that it is a `UiState`-tracked panel.
- Whether `PI_STUDIO_LISTEN` handling (env wins) should be replicated: verified in `packages/server/src/config/daemon-config.ts` `overlayEnv` (`PI_STUDIO_LISTEN` → `daemon.listen`); confirm the daemon bootstrap path (`daemon/bootstrap.ts`) uses `overlayEnv` too, and whether the `config.json` listen key can hold forms beyond `host:port` (e.g. a unix socket path).
- Whether the 50-entry cap, 64-char name and `localDetectionDismissed` flag (decisions of this spec/config spec) are acceptable.
