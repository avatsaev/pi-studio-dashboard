# Wire fixtures

| Directory | Owner | Contents |
|---|---|---|
| `pi-studio/` | pi-studio | Verbatim copy of `packages/protocol/fixtures/external-clients/`. Provenance in `SOURCE.json`. |
| `local/` | this repo | Hand-written degraded frames (unknown kind/status/method, missing feature flags) in the same file format, with their own `manifest.json`. |

## Re-vendor procedure

1. From the repo root, with `../pi-studio` checked out at the commit to vendor:
   ```sh
   rsync -a --delete ../pi-studio/packages/protocol/fixtures/external-clients/ crates/protocol/tests/fixtures/pi-studio/
   ```
   (`cp -a` into an emptied directory works too; hidden files such as `.signatures.json` must come along.)
2. Update `SOURCE.json`: `commit` (`git -C ../pi-studio rev-parse HEAD`), `version`, `vendoredOn`.
3. Read `pi-studio/CHANGELOG.md` for entries newer than the previous `version`; a breaking entry means a
   `formatVersion` bump and consumer migration work.
4. Run `cargo test -p pistudiodashboard-testkit` (manifest/file consistency, frame decoding,
   `formatVersion` guard) and fix any typed decoders that now fail.
5. Verify: `diff -r crates/protocol/tests/fixtures/pi-studio ../pi-studio/packages/protocol/fixtures/external-clients` is empty.

## Rules

- Never edit files under `pi-studio/` by hand; change them in pi-studio and re-vendor.
- `local/` is dashboard-owned: a re-vendor never touches or overwrites it.
- Fixtures contain no secrets or real hosts; keep it that way in `local/`.
