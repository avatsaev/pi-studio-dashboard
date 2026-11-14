# Task 002 — Vendor pi-studio fixtures and the fixture harness

- **Sprint:** sprints/sprint-001-core-crates
- **Status:** done
- **Type:** test
- **Area:** L0 fixtures, `crates/testkit`
- **Priority:** P1
- **Estimated size:** S
- **Depends on:** task-001

## Goal
Vendor pi-studio's external-client fixtures verbatim into the repo, add a testkit loader for `manifest.json`, and test that the manifest matches the files, every socket frame parses through the task-001 decoder, and a newer `formatVersion` fails loudly.

## Context / why
The fixtures are the wire contract (D22, D37): every later wire type, reducer and client test deserialises them. Vendoring them now, with a recorded source commit, lets sprints 004–010 add typed decoders against real frames. The re-vendor procedure must be written down so a pi-studio change is a reviewable diff.

## Scope references
- `swe/specs/architecture/protocol-client.md` § Data & persistence touchpoints (fixtures and the `tests/fixtures/local/` degraded set)
- `swe/specs/overview.md` § Delivery stages
- Source: `../pi-studio/packages/protocol/fixtures/external-clients/` at pi-studio commit `47d80e6` (version `0.0.115`, committed 2026-11-13); its `README.md` describes the file format.
- Create: `crates/protocol/tests/fixtures/pi-studio/**` (verbatim copy incl. `manifest.json`, `README.md`, `CHANGELOG.md`, dotfiles such as `.signatures.json`), `crates/protocol/tests/fixtures/SOURCE.json`, `crates/protocol/tests/fixtures/local/`, `crates/protocol/tests/fixtures/README.md`
- Create: `crates/testkit/src/fixtures.rs` (+ `pub mod fixtures;` in `crates/testkit/src/lib.rs`), `crates/testkit/tests/vendored_fixtures.rs`

## What to build
- Copy the directory with `cp -a`/`rsync` (nothing edited, nothing reformatted, hidden files included). `SOURCE.json`: `{ "repo": "pi-studio", "path": "packages/protocol/fixtures/external-clients", "commit": "47d80e6…(full hash)", "version": "0.0.115", "vendoredOn": "<date>" }`.
- `crates/protocol/tests/fixtures/local/`: hand-written degraded fixtures in the **same file format** (`{type, direction, description, since, stage, requiresFeature, volatile, frame}`): `status.server_info.no_observability.json` (features without `daemonObservability`, plus `daemonStatusRpc: true` to prove it is ignored later), `agent_stream.unknown_kind.json`, `agent_update.unknown_status.json`, `agent_ui_request.unknown_method.json`. A re-vendor never touches this directory. Its own `manifest.json` lists them (same schema, `formatVersion: 1`).
- `crates/protocol/tests/fixtures/README.md`: re-vendor procedure (exact copy command from `../pi-studio`, update `SOURCE.json`, run the harness, read pi-studio's fixture `CHANGELOG.md` for breaking entries, never edit vendored files by hand, `local/` is dashboard-owned).
- Testkit loader (`testkit::fixtures`, serde_json + std::fs only; testkit stays free of workspace *normal* dependencies):
  - `pub struct Manifest { format_version: u32, messages: Vec<ManifestEntry> }`, `ManifestEntry { file, type_, direction, response: Option<String>, exception: Option<bool|String as the source defines> }` (read the source `manifest.json` and README to type `exception` exactly).
  - `pub struct Fixture { type_, direction, since, requires_feature, volatile: Vec<String>, frame: Value }`.
  - `pub fn load_manifest(dir: &Path) -> Result<Manifest, FixtureError>` (rejects `formatVersion > SUPPORTED_FORMAT_VERSION = 1` with a message naming both versions), `pub fn load_fixture(dir, &ManifestEntry)`, `pub fn pi_studio_dir()` / `pub fn local_dir()` resolving `../protocol/tests/fixtures/{pi-studio,local}` from `CARGO_MANIFEST_DIR`. Errors are typed; no unwrap/expect/panic outside `#[cfg(test)]`.
- Test `crates/testkit/tests/vendored_fixtures.rs` (testkit gains `pistudiodashboard-protocol` as a **dev-dependency**; legal because testkit is above L0 and `protocol` stays dependency-free):
  1. manifest ↔ directory: every manifest `file` exists, every `*.json` (except `manifest.json`) is listed, each `response` names a listed file;
  2. every fixture whose `direction` is `server_to_client`/`client_to_server` has a `frame` that `protocol::decode_frame` accepts (re-serialised to text); the `http` fixture (`health.json`, frame `{status, contentType, body}`) is excluded from this check and asserted to have those three keys;
  3. a manifest with `formatVersion: 2` (built in a temp dir) makes `load_manifest` fail with the version message;
  4. `local/` fixtures load through the same loader and decode as frames.

## Out of scope
- Typed decoding of individual messages (sprint-004/task-001 and later add per-type tests over these fixtures).
- Replaying fixtures through reducers or a fake daemon (sprint-004/task-002, sprint-006).
- Any xtask step that fetches pi-studio; vendoring stays a documented manual procedure.

## Acceptance criteria
- [ ] Given the vendored directory, when compared with `../pi-studio/packages/protocol/fixtures/external-clients/` at `47d80e6`, then it is byte-identical (e.g. `diff -r` clean), including hidden files.
- [ ] Given the harness, when run, then manifest and files are consistent in both directions and every non-`http` frame decodes via task-001 (`Frame::Session` for session frames, bare `status`/`ping`/`pong`/`hello` for the rest).
- [ ] Given a manifest with `formatVersion` 2, when loaded, then loading fails with an error naming the supported version (1) — not silently accepted.
- [ ] Given `tests/fixtures/local/`, then the four degraded fixtures exist, are in their own manifest, load and decode as frames, and `SOURCE.json` records commit, version and date.
- [ ] Given `crates/protocol/tests/fixtures/README.md`, then it states the re-vendor steps and that `local/` is never overwritten.

## Test / verification plan
- Tests: `cargo test -p pistudiodashboard-testkit`; the vendored-fixtures test must be non-ignored (fast).
- Layers: `cargo xtask layers` (testkit → protocol is dev-only; protocol still has no workspace deps).
- Gate: `cargo xtask ci` passes.

## Notes
- Outline problem resolved here: `Class::Standalone` forbids any workspace dependency (dev included), so `protocol` cannot dev-depend on testkit; the harness test therefore lives in testkit with `protocol` as a dev-dependency. Update the doc comment in `crates/testkit/src/lib.rs` ("depends on no workspace crate") to say "no workspace crate as a normal dependency".
- `agent_stream.*` fixtures use ISO timestamps; `pong`/`agent_ui_*` use epoch ms — nothing to normalise in this task.
- Fixtures are pi-studio's, copyright avatsaev (MIT OR Apache-2.0 per D20); no `ATTRIBUTION.md` row needed for test data, but note provenance in `SOURCE.json`.
- Fixtures contain no secrets by pi-studio's contract; do not add real hosts or tokens to `local/`.
