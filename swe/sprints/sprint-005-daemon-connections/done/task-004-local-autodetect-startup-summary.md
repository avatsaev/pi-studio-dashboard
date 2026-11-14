# Task 004 — Local auto-detect and startup — Summary

- **Sprint:** sprints/sprint-005-daemon-connections
- **Completed:** 2026-11-14
- **Status:** done

## What was implemented
`detect_local.rs`: `detect(env, user_home)` per the spec pseudocode (PI_STUDIO_HOME or ~/.pi-studio, default 127.0.0.1:6767, 1 MiB cap, env `PI_STUDIO_LISTEN` wins over the file, `host:port` and `[::1]:port`, wildcard -> 127.0.0.1, every failure -> default, no error). `startup.rs`: `config::load` (corrupt file renamed to `config.json.bak` by the loader), toasts for a corrupt file / newer-version (read-only) / repaired values, `Session::apply_loaded_config`, auto-add/refresh rule (add when no auto-detected entry, no entry at the same `host:port` and not dismissed; refresh an untouched auto-detected entry whose address changed via `daemon.edit {autoDetected:true}`; leave edited entries alone), then `connect_all` (each entry with its own password lookup; entries linked during the auto-add are not connected twice). Shell command `daemon.detectLocal` (Daemon menu; hook + "no auto-detected entry yet" as enabled rules; clears `localDetectionDismissed` through `daemon.add`). Shell `Services.detect_local_daemon` hook. Config dir resolution shared by app and CLI: `config::config_dir` (Linux `$XDG_CONFIG_HOME` or `~/.config`, macOS Application Support, Windows %APPDATA%, plus `com.avatsaev.pistudiodashboard`); `--config-dir`/`PISTUDIODASHBOARD_CONFIG_DIR` override. Dir 0700 / file 0600 from `config::save`.

## Deviations
- The config dir is computed by `config::config_dir` rather than Tauri's path API (same layout; TODO(verify) against `app_config_dir` on macOS/Windows), so headless runs and the CLI resolve the same file.
- Single-instance protection is not part of this task (TODO(verify) remains).

## Build & test results
`cargo test -p pistudiodashboard`: detect table (env override, file, wildcard, IPv6, bad file, bad types, size cap, home default), startup scenarios (empty config adds Local once and connects once; truncated and garbage file -> `.bak`, empty list, toast, re-detected; dismissed -> nothing; untouched entry follows the address, edited entry untouched; newer version read-only, file bytes unchanged; two entries both connect). Shell test for `daemon.detectLocal`. `cargo test -p pistudiodashboard-config` (config_dir).

## Acceptance criteria
All seven are covered by the tests above except the 2-second visibility, which the smoke shows.

## Smoke against pi-studio
Against the real dev daemon (127.0.0.1:6790, mock provider) with the headless app: `PI_STUDIO_LISTEN=127.0.0.1:6790 ./target/debug/pistudiodashboard --headless --control 7878 --control-token-file /tmp/pistudio-smoke/control.token --config-dir /tmp/pistudio-smoke/cfg`, driven over the control socket (`ui.inspect`, `engine.execute`). Empty config dir + `PI_STUDIO_LISTEN=127.0.0.1:6790`: within ~4 s of launch (the log shows probing..live in under a second) the entry `Local` (`127.0.0.1:6790`, ws, `autoDetected:true`, web UI `http://localhost:4173`, `lastSeenServerId` remembered after Hello) existed in `cfg/config.json` (mode 0600, dir 0700) and 11 agent cards (+ pending dialogs as attention) were in `document.inspect`. Closing the loop: a closed port entry reconnect/backoff behaviour is in task-002's smoke.
