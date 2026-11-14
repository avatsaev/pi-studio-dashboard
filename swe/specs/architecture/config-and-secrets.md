# Config & Secrets — dashboard config file (L3) and keychain

> Part of: [overview.md](../overview.md)
> Dependencies: [app-structure.md](app-structure.md) (where the config dir is resolved, `Services` hooks), [protocol-client.md](protocol-client.md) (consumes entries + passwords), [../features/daemon-connections.md](../features/daemon-connections.md), [../features/notifications-and-tray.md](../features/notifications-and-tray.md) (settings consumers)
> Source brief: [daemon-monitoring-dashboard](../../grills/daemon-monitoring-dashboard.md) — D8, D9, D14, D18, D21, D29, D32

## Purpose

Defines the only thing the dashboard persists (D8): the daemon list and a few settings, in a versioned JSON file owned by `crates/config` (`pistudiodashboard-config`, L3, replaces the sample `format` crate), plus the policy for daemon passwords, which live only in the OS keychain (D18). Follows the preferences pattern (`av-rust-desktop-grow/patterns/06-preferences.md`): a serde struct changed through validating commands; **lenient loading** — an unreadable or invalid value falls back to its default and is reported; a corrupt file never stops the app from starting; the directory is resolved in the app binary and passed in; file access is `cfg(not(target_arch = "wasm32"))` so the crate still builds for wasm (parse/validate/serialize are pure).

## Public contract

### Location

| Item | Value |
|------|-------|
| Directory | Tauri app config dir for identifier `com.avatsaev.pistudiodashboard` (resolved in `apps/pistudiodashboard/src-tauri` with Tauri's path API and passed to L3 as a path; Linux `~/.config/com.avatsaev.pistudiodashboard/`, macOS `~/Library/Application Support/com.avatsaev.pistudiodashboard/` — TODO(verify) exact Tauri 2 resolution) |
| File | `config.json` |
| Backup of corrupt file | `config.json.bak` (single slot; replaced if a later corruption happens; see recovery) |
| Temp file | `config.json.tmp` in the same directory (atomic write) |
| Own app log | separate file in the app log dir (not in this spec), never containing secrets (D29, D32) |
| Permissions | File mode `0600` on Unix, directory created `0700` (decision of this spec: it names hosts). |

### File format (`version: 1`)

```json
{
  "version": 1,
  "daemons": [
    { "entryId": "uuid-v4", "name": "Local", "host": "127.0.0.1", "port": 6767,
      "tls": false, "webUiUrl": "http://localhost:4173",
      "lastSeenServerId": "abc", "autoDetected": true }
  ],
  "settings": {
    "theme": "system",
    "notifications": { "needsInput": true, "failures": true },
    "closeToTray": true,
    "attentionRailCollapsed": false
  },
  "localDetectionDismissed": false
}
```

Unknown top-level keys and unknown keys inside entries/settings are **preserved** on rewrite (an `extra` map per object) so a newer app version's data is not destroyed by an older one (decision of this spec). The password is not a field anywhere (D18).

### Field schema and caps (all caps are decisions of this spec unless D-cited)

| Field | Type | Default | Validation / cap |
|-------|------|---------|------------------|
| `version` | integer | `1` | Must be ≥ 1 and ≤ supported max; larger → refuse to overwrite (see Migration). |
| `daemons` | array | `[]` | ≤ 50 entries (brief target is 10 daemons, D28; 50 gives headroom). Extra entries on load are dropped and reported. Order = display order. |
| `daemons[].entryId` | string | generated | UUID v4 format, unique. Duplicate or invalid → regenerated + reported (breaks keychain link; acceptable, user re-enters password). |
| `daemons[].name` | string | — | 1–64 chars after trim, no control chars, unique case-insensitively (duplicates on load get a " (2)" suffix). |
| `daemons[].host` | string | — | 1–253 chars; same grammar as [daemon-connections.md](../features/daemon-connections.md). Invalid entry on load → entry dropped + reported (never connects to garbage). |
| `daemons[].port` | integer | 6767 | 1–65535; invalid → entry dropped + reported. |
| `daemons[].tls` | bool | `false` | |
| `daemons[].webUiUrl` | string? | none | http(s) only, ≤ 2048 chars, no userinfo; invalid → `None` + reported. (D24) |
| `daemons[].lastSeenServerId` | string? | none | ≤ 128 chars; used for D19 identity-change detection. |
| `daemons[].autoDetected` | bool | `false` | (D10) |
| `settings.theme` | enum | `"system"` | `dark` \| `light` \| `system` (D21); unknown → default. |
| `settings.notifications.needsInput` | bool | `true` | D9, D36, D37. Covers `agent_ui_request` dialogs with `expectsResponse` only (no other attention source). |
| `settings.notifications.failures` | bool | `true` | D9. |
| `settings.closeToTray` | bool | `true` | D14. |
| `settings.attentionRailCollapsed` | bool | `false` | Persisted UI preference (decision of this spec: the rail's collapsed state is a user expectation that survives restart); filters, selection, open panels are **not** persisted (D8). |
| `settings.zoom` | integer | `100` | Whole percent `50`–`300` (view-zoom Z2); anything else → default + issue reported, file still loads. |
| `settings.groupBy` | enum | `"workspace"` | `none` \| `workspace` (agent-workspace-grouping W3); unknown → default + issue reported, file still loads. |
| `localDetectionDismissed` | bool | `false` | Set when the user removes the auto-detected entry, so it is not recreated (see daemon-connections). |
| file size | | | > 1 MiB → treated as corrupt (hostile/garbage input). |

Window geometry is platform state and is persisted by a Tauri plug-in, not in this file (pattern 06).

### API (Rust, L3 — names are a contract for other specs)

| Item | Intent |
|------|--------|
| `Config { version, daemons: Vec<DaemonEntry>, settings: Settings, local_detection_dismissed, extra }` | Plain serde data. |
| `parse(bytes) -> ParseOutcome { config, issues: Vec<ConfigIssue> }` | Lenient, per-field fallback; never panics; pure. |
| `validate_entry(&DaemonEntry, &Config) -> Result<(), Vec<FieldError>>` | Used by `daemon.add/edit` and by `parse`. Field-keyed errors. |
| `serialize(&Config) -> Vec<u8>` | Stable key order, pretty-printed with 2-space indent (hand-editable). |
| `load(dir) -> LoadOutcome { config, issues, recovered_from_corrupt: bool }` | cfg(not wasm32). |
| `save(dir, &Config) -> Result<()>` | Atomic write, cfg(not wasm32). |
| `ConfigIssue { kind, path, detail }` | `kind` ∈ `dropped_entry`, `defaulted_value`, `capped`, `corrupt_file`, `future_version`, `io`. |

The engine holds `Config` in its session; commands `daemon.add/edit/remove/setPassword`, `view.setTheme`, `settings.setNotification`, `settings.setCloseToTray`, `view.toggleAttentionRail` mutate it and request a save through a `Services::save_config` hook (debounced 250 ms, coalescing; flush on exit). The engine never touches the file system itself (pattern 06). CLI automation: `apps/pistudiodashboard-cli` can read/validate a config file offline with the same crate (`config validate <path>`, `config show` — passwords never shown).

## Behavior & algorithms

### Load and recovery (D29)

```
function load(dir):
    if file absent: return default Config (empty daemons), no issue     # first run
    bytes = read(file); on I/O error: issues += io; return defaults, DO NOT overwrite the file
    if len(bytes) > 1 MiB or not valid UTF-8 JSON object:
        rename config.json → config.json.bak   (replacing an old .bak)
        issues += corrupt_file ; return default Config, recovered_from_corrupt = true
    outcome = parse(bytes)            # per-field leniency: bad values → defaults + issue
    if outcome.config.version > SUPPORTED:
        issues += future_version      # keep running with what was understood, set read_only = true:
        return (config, read_only)    # never save over a newer file
    return outcome
```

"Corrupt" means JSON that fails to parse or whose top level is not an object. A file that is valid JSON but has bad individual values is NOT corrupt: those values default and are reported (no `.bak`), which satisfies "an invalid value must not fail the whole file". A truncated file and garbage both take the corrupt path. The recovery warning ("Your settings file was unreadable; it was saved as config.json.bak and the dashboard started empty") is shown once as a toast and in diagnostics; the local daemon is then re-detected (D10) so the app is useful immediately.

### Atomic write

```
function save(dir, config):
    ensure dir exists (0700)
    write bytes to config.json.tmp (0600), fsync file
    rename tmp → config.json     # atomic replace on same filesystem
    best-effort fsync dir
on any error: keep the old file, report issues += io, keep in-memory config, retry on next change; surface a non-blocking toast once.
```

If the previous good `config.json` exists and is about to be replaced, no extra backup is made (the `.bak` slot is reserved for corrupt files only, decision of this spec). Concurrent app instances: not supported; the second launch is blocked by Tauri single-instance (app-structure spec; TODO(verify) plug-in choice) so two writers never exist.

### Keychain and password policy (D18)

| Item | Value |
|------|-------|
| Crate | `keyring` (candidate for the pinned major; licence MIT/Apache-2.0 — TODO(verify) per golden rule 9, plus the platform backend features chosen: Linux secret-service/keyutils, macOS Keychain) |
| Service | `com.avatsaev.pistudiodashboard` |
| Account | `daemon:<entryId>` |
| Value | the password, UTF-8 string, ≤ 1,024 bytes |
| Access point | app binary only (`Services` keychain hook: `get(entryId)`, `set(entryId, secret)`, `delete(entryId)`); never the engine, never the webview (rule 6) |

```
password_for(entry):
    r = keychain.get(entry.entryId)
    ok(secret)            → use
    no_entry              → none (connect without; 401 triggers prompt, D25)
    locked/denied/no backend/any other error → none + flag keychain_unavailable (D29); prompt happens after a 401
set(entry, secret, remember):
    if remember and keychain.set ok → stored
    else → hold in an in-memory map entryId → secret (process lifetime), warn "not saved" (D18)
on daemon.remove: keychain.delete(entry.entryId) (missing item is fine), drop the in-memory copy
on daemon.setPassword with empty + remember → keychain.delete
```

The in-memory map uses zeroising secret strings, is never serialised, never part of `Config`, view, `UiState`, `ui.inspect`, snapshots, notifications, or logs (D32, D29). The dialog password field is write-only: the view contains only `hasPassword: bool` and `passwordInMemoryOnly: bool` per entry (D32: "passwords never reach the webview" — the secret travels webview→Rust once on submit as the command argument; it is never echoed back; TODO(verify) with app-structure whether the registry command arg logging in the control protocol redacts `password` arguments; the control-protocol log MUST redact `daemon.add`/`daemon.setPassword` `password` args). Logging rules: the `config` and app crates MUST NOT log secrets or full entry JSON with passwords; error values containing keychain details are logged without the secret.

Orphaned keychain items (entries removed while the keychain was locked) are not swept in v1; on `daemon.remove` failure to delete, the app warns and continues (TODO(verify) whether to retry on next start).

### Settings commands (validation)

| Command | Validation | Persistence |
|---------|-----------|-------------|
| `view.setTheme {theme}` | `dark\|light\|system` else `invalid_theme` | `settings.theme` |
| `settings.setNotification {kind, enabled}` | `kind` ∈ `needsInput\|failures` else `invalid_kind` | `settings.notifications.<kind>` |
| `settings.setCloseToTray {enabled}` | bool | `settings.closeToTray` |
| `view.toggleAttentionRail` | none | `settings.attentionRailCollapsed` |
| `view.zoomIn` / `view.zoomOut` / `view.zoomReset` | none (refused at the ends with a reason) | `settings.zoom` |
| `view.setGroupBy {groupBy}` | `none\|workspace` else the error `` `groupBy` is one of: none, workspace `` (shell commands return a message, like `view.setTheme`) | `settings.groupBy` |
| `daemon.add/edit/remove/setPassword` | see daemon-connections | `daemons` / keychain |

Each persisted change saves via the debounced hook; notification and tray toggles take effect immediately in the app binary (mirrored into `UiState`).

### Migration

Fresh app: nothing to migrate from (D8). `version: 1` is current. Rule for the future: a loader recognises older versions and upgrades in memory, saving only after a successful upgrade, and never saves over a newer `version` (read-only mode above).

## Data & persistence touchpoints

- Writes: `config.json`, `config.json.tmp`, `config.json.bak` in the config dir; keychain items for service `com.avatsaev.pistudiodashboard`. Nothing else is persisted — no agents, timelines, dialogs, usage (D8).
- Reads: nothing from pi-studio's files here (the local-detection read is in daemon-connections).
- Not persisted: filters, selection, open panels, drill-down, connection state, in-memory passwords.

## Error handling & edge cases

| Condition | Expected behavior |
|-----------|-------------------|
| Config absent | Defaults; local daemon auto-detected; file created on first change. |
| Truncated / garbage / non-object JSON | Renamed to `.bak`, empty config, warning, app starts (D29). |
| Valid JSON, bad value (`port: 99999`, `theme: "neon"`) | Entry dropped or value defaulted, issue reported; file not renamed. |
| > 50 entries | First 50 kept, rest dropped + issue `capped`. |
| `.bak` already exists | Overwritten by the newest corrupt file. |
| Read-only directory / disk full | Save fails, in-memory config remains, one toast, retry on next change. |
| Config from a newer app version | Read-only mode, banner "settings from a newer version; changes won't be saved". |
| Keychain locked/denied/unavailable | Treated as no password + prompt after 401, nothing on disk (D18, D29). |
| `daemon.remove` with failing keychain delete | Entry removed; warning toast. |
| Secret in a crash/panic message | Passwords are held in wrapper types whose `Debug`/`Display` print `***`. |
| Two entries sharing `entryId` after hand edit | Second regenerated + issue. |
| Hand-edited file containing a `password` key | Ignored and never read, never rewritten; issue `defaulted_value` noting plaintext passwords are unsupported (D18). |

## Dependencies on other specs

- [app-structure.md](app-structure.md) — config dir resolution, `Services` hooks (`save_config`, keychain), single-instance, control-protocol redaction.
- [protocol-client.md](protocol-client.md) — consumes `DaemonEntry` and passwords.
- [daemon-connections.md](../features/daemon-connections.md) — entry validation grammar, local detection, identity rules.
- [notifications-and-tray.md](../features/notifications-and-tray.md) — consumers of the notification/tray settings.
- [design-system.md](design-system.md) — theme values.

## Acceptance criteria

- [ ] Given no config file, when the app starts, then it runs with defaults (`theme: system`, both notification kinds on, `closeToTray: true`) and no warning. (D8, D9, D14, D21)
- [ ] Given a truncated config file and a garbage file (separately), when the app starts, then the original is renamed to `config.json.bak`, the app starts with an empty daemon list, a warning is shown, and the app is usable. (D29)
- [ ] Given a valid config with `theme:"neon"` and one entry with `port:70000`, then the theme defaults, that entry is dropped, the other entries load, issues are reported and no `.bak` is created. (D29, pattern 06)
- [ ] Given a settings change, when saved, then the file is replaced atomically (a simulated crash between tmp write and rename leaves the previous file intact and parseable) and has mode 0600. 
- [ ] Given a daemon entry with a password, then `config.json` contains no password and the keychain holds service `com.avatsaev.pistudiodashboard` account `daemon:<entryId>`. (D18)
- [ ] Given an unavailable or locked keychain, when `daemon.setPassword {remember:true}` runs, then the password is held in memory only, a "not saved" warning is returned and nothing is written to disk. (D18, D29)
- [ ] Given `daemon.remove`, then the keychain item for that `entryId` is deleted and the entry is gone from the file. (D18)
- [ ] Given a captured view, `ui.inspect` output, app log, and notification payloads during add/connect/auth-failure, then none contains the password. (D32, D29)
- [ ] Given a config with 51 entries, then 50 load and an issue `capped` is reported.
- [ ] Given a config `version` higher than supported, then the app runs read-only and never rewrites that file.
- [ ] Given `cargo xtask wasm`, then `pistudiodashboard-config` builds for wasm32 (file I/O gated). (rule 7)
- [ ] Given `config validate <path>` on the CLI, then issues are listed and no password is printed.

## TODO(verify)

- Tauri 2 path API resolution of `com.avatsaev.pistudiodashboard` per OS, and confirm the identifier in `tauri.conf.json`.
- `keyring` crate licence, pinned version and platform backend features (Linux headless/no secret-service behaviour → must return an error, not hang).
- Whether the control protocol / `ui.inspect` logs command arguments, and redaction of `password` args.
- Single-instance enforcement plug-in (so two processes never write the file).
- Whether `attentionRailCollapsed` should persist (this spec proposes yes), and the 50-entry / 64-char caps (decisions of this spec).
