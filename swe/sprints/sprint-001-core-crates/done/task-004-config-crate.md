# Task 004 — Config crate: schema, lenient load, atomic save

- **Sprint:** sprints/sprint-001-core-crates
- **Status:** done
- **Type:** feature
- **Area:** L3 `crates/config` (dashboard config file)
- **Priority:** P1
- **Estimated size:** M
- **Depends on:** none

## Goal
Create `pistudiodashboard-config` (L3): the versioned `config.json` model (daemon entries + settings), lenient `parse`, `validate_entry`, stable `serialize`, `load` with `.bak` recovery, and atomic `save` — all file I/O behind `cfg(not(target_arch = "wasm32"))`.

## Context / why
The config file is the only thing the dashboard persists (D8, D18). A corrupt or hand-edited file must never stop the app from starting (D29), a newer app's data must never be destroyed, and no password is ever a field. The engine (sprint-005/task-002) and startup (sprint-005/task-004) build on this API; the sample `format` crate it replaces is deleted in sprint-002/task-002.

## Scope references
- `swe/specs/architecture/config-and-secrets.md` § Location, § File format, § Field schema and caps, § API, § Load and recovery, § Atomic write, § Settings commands (validation rules only), § Migration, § Error handling & edge cases, § Acceptance criteria (all but keychain/CLI/`ui.inspect` items)
- `swe/specs/features/daemon-connections.md` § Behavior (host grammar used by `validate_entry`; read it before writing the validator)
- `swe/grills/daemon-monitoring-dashboard.md` D8, D9, D14, D18, D21, D24, D29, D32
- Style reference: `crates/format/src/lib.rs` (hostile-file tests, `decode`), replaced later
- Create `crates/config/{Cargo.toml,src/lib.rs,src/model.rs,src/parse.rs,src/validate.rs,src/io.rs}`; register `("config", Class::Layer(3))` in `xtask/src/layers.rs` `TABLE`; add to `[workspace.dependencies]`

## What to build
- Dependencies: `serde`, `serde_json`, `thiserror`; **no** workspace dependency unless the host validator is shared with `protocol` (spec allows none — prefer none). `uuid`-style entry ids: generate a v4-format id **without a new crate** by taking a `fn(&mut [u8;16])`/seed supplied by the caller (`new_entry_id(random: [u8;16]) -> String`) so the crate stays wasm-clean; the app binary supplies randomness (it already depends on `getrandom`). If a third-party crate is nevertheless added, check and record its licence (rule 9).
- Public API exactly as the spec table (names are a contract for later sprints):
  - `Config { version: u32, daemons: Vec<DaemonEntry>, settings: Settings, local_detection_dismissed: bool, extra: Map }`; `DaemonEntry { entry_id, name, host, port: u16, tls, web_ui_url: Option<String>, last_seen_server_id: Option<String>, auto_detected, extra }`; `Settings { theme: ThemeChoice{Dark,Light,System}, notifications: Notifications{needs_input, failures}, close_to_tray, attention_rail_collapsed, extra }`; defaults: `theme: system`, both notifications `true`, `closeToTray: true`, rail not collapsed. Unknown keys at every level are preserved in `extra` and re-emitted by `serialize`. A `password` key is never read nor re-emitted; it adds `ConfigIssue{defaulted_value}` noting plaintext passwords are unsupported.
  - `parse(&[u8]) -> ParseOutcome { config, issues }` — pure, never panics, per-field leniency; `serialize(&Config) -> Vec<u8>` — 2-space pretty JSON, stable key order.
  - `validate_entry(&DaemonEntry, &Config) -> Result<(), Vec<FieldError>>` — name 1–64 chars after trim, no control chars, unique case-insensitive; host 1–253 chars per the daemon-connections grammar; port 1–65535; `webUiUrl` http(s) only, ≤ 2048 chars, no userinfo; `lastSeenServerId` ≤ 128. Field-keyed errors (`FieldError { field, message }`).
  - Caps applied by `parse`: > 50 daemons → first 50 kept + `capped` issue; invalid host/port entry dropped + `dropped_entry`; invalid `webUiUrl` → `None` + issue; duplicate/invalid `entryId` regenerated (needs the id source: `parse` takes a `&mut dyn FnMut() -> String` or returns entries flagged for regeneration — choose and document) + issue; duplicate names get a ` (2)` suffix; unknown theme → default + issue.
  - `ConfigIssue { kind: DroppedEntry|DefaultedValue|Capped|CorruptFile|FutureVersion|Io, path: String, detail: String }`.
  - `load(dir: &Path) -> LoadOutcome { config, issues, recovered_from_corrupt, read_only }` (`cfg(not(wasm32))`): absent file → defaults, no issue; I/O error → defaults + `Io`, **do not overwrite**; > 1 MiB, invalid UTF-8/JSON, or non-object → rename `config.json` to `config.json.bak` (replacing an old `.bak`), `CorruptFile`, defaults, `recovered_from_corrupt = true`; `version` > `SUPPORTED_VERSION` (1) → `FutureVersion`, `read_only = true`; `save` refuses while read-only (the caller holds the flag; `save(dir, &Config, read_only)` or a `Store` wrapper — pick one and document).
  - `save(dir, &Config) -> Result<(), ConfigError>` (`cfg(not(wasm32))`): create dir `0700` on Unix, write `config.json.tmp` mode `0600`, `fsync`, `rename` over `config.json`, best-effort dir fsync; on error the old file stays.
- Wasm: everything except `load`/`save` and the `std::fs` imports compiles for `wasm32-unknown-unknown`.
- Do not log anything containing entry JSON; `Debug` of entries is fine (no secrets exist in this crate).

## Out of scope
- Keychain/password handling and `Services` hooks → sprint-005/task-003 and sprint-005/task-001 (`config_store` hook wiring in task-002 of sprint-005).
- Engine commands that mutate config (`daemon.*`, `settings.*`), the debounced save, startup load sequence and the recovery toast → sprint-005/task-002, task-004; `view.setTheme`/settings commands → sprint-002/task-003 and sprint-008.
- Local-daemon auto-detection → sprint-005/task-004.
- CLI `config validate`/`config show` (spec § API, last paragraph; AC "CLI … no password printed") — **not scheduled in the outline**; see Notes.
- Single-instance enforcement.

## Acceptance criteria
- [ ] Given no config file, when `load` runs, then it returns defaults (`theme: system`, notifications on, `closeToTray: true`, empty daemons) and no issue.
- [ ] Given a truncated file and a garbage file (separately), when `load` runs, then the original is renamed to `config.json.bak`, defaults are returned, `recovered_from_corrupt` is true, and `CorruptFile` is reported; an existing `.bak` is replaced.
- [ ] Given valid JSON with `theme:"neon"` and one entry `port:70000` beside a good entry, then the theme defaults, the bad entry is dropped, the good one loads, issues are reported, and **no** `.bak` is created.
- [ ] Given 51 entries, then 50 load and a `Capped` issue is reported.
- [ ] Given `version: 2`, then `read_only` is true, the understood fields load, and a save attempt is refused with the file untouched.
- [ ] Given a file with unknown top-level, entry-level and settings keys and a `password` key in an entry, when parsed and serialised, then unknown keys survive, the password is gone, and an issue notes plaintext passwords are unsupported.
- [ ] Given a settings change saved via `save`, then `config.json` is replaced atomically, has mode `0600` (Unix), a pre-existing good file is intact if the temp write fails (simulate by a read-only target dir), and `config.json.tmp` does not linger after success.
- [ ] Given `validate_entry` with empty name, 65-char name, duplicate name (different case), host with a scheme or spaces, port 0, `webUiUrl` `ftp://…`/with userinfo/over 2048 chars, then each yields a field-keyed error; a valid entry passes.
- [ ] Given a > 1 MiB file, then it is treated as corrupt.
- [ ] `cargo xtask wasm` builds the crate; `cargo xtask layers` accepts it as L3.

## Test / verification plan
- Build: `cargo build -p pistudiodashboard-config`.
- Tests: crate unit tests plus `crates/config/tests/files.rs` using a temp dir (`tempfile` is already a dev-dependency elsewhere in the workspace — check `Cargo.lock`; if it must be added, check its licence MIT/Apache-2.0 and record it). Cover the criteria above; keep `parse` hostile-input tests table-driven.
- Focused: `cargo test -p pistudiodashboard-config`.
- Gate: `cargo xtask ci` passes.

## Notes
- TODO(verify) (config-and-secrets.md): exact Tauri 2 config-dir resolution per OS and the identifier in `tauri.conf.json` — resolved in sprint-005/task-004 (app binary); here the directory is a caller-supplied `&Path`.
- TODO(verify): whether `attentionRailCollapsed` should persist (spec proposes yes; implemented as specified) and the 50-entry / 64-char caps (decisions of the spec).
- Outline gap: the CLI offline config commands (`config validate <path>`, `config show`) required by the config spec's last AC and app-structure § CLI scope are not in any sprint. sprint-002/task-002 keeps the CLI at `commands` only; flag to Main so a task can be added after this crate exists.
- Atomic-write crash test: simulate "crash between tmp write and rename" by writing the tmp file manually and asserting the old `config.json` still parses.
