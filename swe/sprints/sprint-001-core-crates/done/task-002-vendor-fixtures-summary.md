# Task 002 — Vendor pi-studio fixtures and the fixture harness — Summary

- **Sprint:** sprints/sprint-001-core-crates
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
pi-studio's external-client fixtures vendored verbatim (63 entries incl. `.signatures.json`, `CHANGELOG.md`, `README.md`), `SOURCE.json`, a dashboard-owned `local/` degraded set with its own manifest, a re-vendor README, `testkit::fixtures` loader and the harness test `crates/testkit/tests/vendored_fixtures.rs`.

## Files created / changed
| File | Change |
|------|--------|
| `crates/protocol/tests/fixtures/pi-studio/**` | created (verbatim copy, `diff -r` clean) |
| `crates/protocol/tests/fixtures/SOURCE.json` | created (commit `47d80e6e1f44…`, version 0.0.115, 2026-11-13) |
| `crates/protocol/tests/fixtures/local/**` | created (4 fixtures + `manifest.json`) |
| `crates/protocol/tests/fixtures/README.md` | created (re-vendor procedure, `local/` never overwritten) |
| `crates/testkit/src/fixtures.rs`, `src/lib.rs`, `Cargo.toml` | loader, `pub mod fixtures;`, doc comment, `protocol` dev-dependency |
| `crates/testkit/tests/vendored_fixtures.rs` | created (5 tests) |

## How it satisfies the scope
Loader parses `serde_json::Value` by hand (no serde derive, no new deps, hand-written typed `FixtureError`). `exception` is a free-form string in the source, typed `Option<String>`. `.signatures.json` (dotfile) is not a fixture and is excluded from the directory-vs-manifest check. Deviation: `SOURCE.json` commit is the full hash of `47d80e6`; `../pi-studio` HEAD has moved to `2581365` but `git diff 47d80e6 HEAD -- packages/protocol/fixtures/external-clients` is empty.

## Build & test results
```
$ cargo test -p pistudiodashboard-testkit      # 5 passed, 0 failed
$ cargo test -p pistudiodashboard-protocol     # 11 + 1 passed, 0 failed
$ cargo clippy -p pistudiodashboard-testkit --all-targets -- -D warnings   # clean
$ cargo fmt -p pistudiodashboard-testkit --check   # clean
$ cargo check -p pistudiodashboard-testkit --target wasm32-unknown-unknown # ok
$ diff -r crates/protocol/tests/fixtures/pi-studio ../pi-studio/packages/protocol/fixtures/external-clients   # empty
$ cargo xtask layers   # fails only: config/protocol "not in the layering table" (task-005 owns TABLE); testkit row unchanged
```

## Acceptance criteria
- [x] Byte-identical vendored dir incl. hidden files
- [x] Manifest/files consistent both directions; every non-http frame decodes (`vendored_manifest_matches_directory`, `vendored_frames_decode`)
- [x] `formatVersion` 2 fails naming supported version 1 (`newer_format_version_fails_loudly`)
- [x] Four local fixtures in own manifest, load and decode; `SOURCE.json` recorded (`local_fixtures_load_and_decode`, `source_json_records_provenance`)
- [x] README states re-vendor steps and `local/` is never overwritten

## Follow-ups / TODO(verify)
- `cargo xtask layers`/`ci` green once task-005 registers protocol/config; testkit→protocol dev-only edge not yet verified by xtask.

## Smoke against pi-studio
Daemon at 127.0.0.1:6790 (running). `$PS run "hello" ...` created an agent (`$PS ls` showed idle + running agents). A throwaway node script (in /tmp, deleted) sent `hello`, `ping`, `list_agents_request`, `fetch_agent_timeline_request{direction:"after"}`, while a second `$PS run "#ui confirm"` created a live agent, and compared live frames to fixtures (same JSON types for every fixture key, literal values unless `volatile`, extra live keys allowed):
```
status.server_info.json OK
pong.json OK
list_agents_response.json OK
fetch_agent_timeline_response.json OK
agent_update.json OK (x2)
```
Remaining reports were my script's naive pairing, not drift: a partial `agent_update` (no `status`/`labels`, partial updates by design), `agent_stream` events of other kinds (`user_message`, `turn_started`) compared against the `assistant_message` fixture, and a different `confirm` dialog text (mock `#ui confirm` sends "Clear session?"; title/message are not fixed). No real drift observed; these kinds/variants were not matched one-to-one, so stream/UI variants remain unverified live.
