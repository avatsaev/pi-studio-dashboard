# Task 003 — Attribution row, icon decision, themed snapshots — Summary

- **Sprint:** sprints/sprint-003-design-system
- **Completed:** 2026-11-13
- **Status:** done

## What was implemented
- `ATTRIBUTION.md`: the D20 provenance row for `apps/pistudiodashboard/src/tokens.css`, with the exact spec text and the real pi-studio commit `2581365399bf166157dc1a7dfdb6ee49df6fbac3` (the commit read in task-001), licence `MIT OR Apache-2.0` relicensed by the copyright holder.
- **Icon decision (recorded): inline SVG for v1.** `package.json` is unchanged by this sprint (dependencies: `@tauri-apps/api`, `react`, `react-dom`; no icon or font package). The two glyphs used (close, alert) are drawn in `src/components/icons.tsx`. No `lucide-react` row is needed. `grep -rIl "lucide\|\.woff\|\.ttf\|@fontsource" src index.html` finds nothing and `src/` has no font/image asset; the base font is the platform stack `--pi-font-ui`.
- Snapshots of the current skeleton in dark and light (below), looked at; no CSS defect left.

## Licence wording check (spec TODO closed)
- Repo root has `LICENSE-MIT` (MIT, "The Pi Studio Dashboard Authors"), `LICENSE-APACHE` (Apache-2.0), `Cargo.toml` `license = "MIT OR Apache-2.0"`, and `NOTICE` says "Licensed under either of the Apache License, Version 2.0 (LICENSE-APACHE) or the MIT license (LICENSE-MIT), at your option." The row's `MIT OR Apache-2.0` matches all four. `NOTICE` needs no change (it names this project's own licence; the row covers the ported values).
- Observed, not resolved here: the sibling `../pi-studio/package.json` declares `"license": "AGPL-3.0"` and has no top-level LICENSE file. D20 rests on the engineer, as the copyright holder, granting the ported design values under `MIT OR Apache-2.0`; the row states that. The git log of pi-studio shows author names besides `avatsaev`/`Aslan Vatsaev` (`Azamat`, `aza`, `p`, `pi-agent`), whose copyright I cannot judge. TODO(verify): the engineer confirms (a) the written grant and (b) that no other contributor owns any ported value.

## Files created / changed
| File | Change |
|------|--------|
| `ATTRIBUTION.md` | provenance row added |
| `swe/sprints/sprint-003-design-system/done/screenshots/s003-t003-*.png` | 6 PNGs |
No source change: the CSS defects found were none. (Snapshot tool, tokens and components were changed in tasks 001/002.)

## Build & test results
```
$ cd apps/pistudiodashboard && npm run check && npm run lint && npm test
tsc clean; oxlint --deny-warnings clean; 14 files, 123 tests passed, 0 failed
$ cd ../.. && cargo xtask web      # the web CI step: check, lint, test, build -> 123 passed, build ok
$ cargo xtask ci
=== ci: fmt ===  FAIL: Diff in crates/model/src/event.rs:90 (`Rejected { error }`, `Interrupt { ok }`, `RpcError { … }` need rustfmt line breaks)
  skip web/clippy/layers/test/panic_hunt/doctest/wasm  ("ci failed at `fmt`")
```
`cargo xtask ci` stops at `fmt` because of in-flight edits to `crates/model/src/event.rs` by another agent, outside this assignment (frontend/ATTRIBUTION only). Not fixed here, as instructed. The `web` step passes on its own (above); the Rust steps were not reached, so they are unverified by me.

## Acceptance criteria
- [x] `ATTRIBUTION.md` has the D20 row for `tokens.css` stating `MIT OR Apache-2.0` with the real pi-studio commit hash; wording matches `LICENSE-MIT`/`LICENSE-APACHE`/`Cargo.toml`/`NOTICE`
- [x] No icon/font library or bundled font in `package.json`/`src`; the decision (inline SVG) is recorded here
- [x] `npm run snapshot` once per theme plus the empty monitor: images exist, render from tokens with legible contrast in both themes (below)
- [ ] `cargo xtask ci` passes — **not green**: fails at `fmt` on `crates/model/src/event.rs` (not this task's files); `web` step green

## Snapshots (rule 5)
Commands (from `apps/pistudiodashboard`, after `npm run build`; saved views derive from the real dev-daemon view of sprint-002/task-004, 7 agents):
```
npm run snapshot -- --view t3-dark.json  --out skeleton-dark.png  --size 960x420   # 2 chips (live, backoff), empty grid, rail, toast, status bar
npm run snapshot -- --view t3-light.json --out skeleton-light.png --size 960x420
npm run snapshot -- --view empty-dark.json  --out empty-dark.png  --size 960x420    # empty monitor
npm run snapshot -- --view empty-light.json --out empty-light.png --size 960x420
npm run snapshot -- --script '[["ui.set", {"theme":"dark"}]]'  --out uiset-dark.png  --size 960x420   # real shell, ui.set path
npm run snapshot -- --script '[["ui.set", {"theme":"light"}]]' --out uiset-light.png --size 960x420   # -> {"ok":true,"result":null}
```
Images in `done/screenshots/`: `s003-t003-skeleton-{dark,light}.png`, `s003-t003-empty-{dark,light}.png`, `s003-t003-uiset-{dark,light}.png`.
Looked at all of them. Checked:
- Contrast: foreground vs `surface0/1` strong in both themes; muted text (`foregroundMuted`) readable on cards, rail and status bar; hairlines (`border`) visible on both, subtle in light by design; `Connected` green and `Reconnecting…` accent pills keep their tint and text colour in both themes (dark pill text is `accentBright`).
- Status bar: 35 px strip on `surface1` with a top hairline and one rounded `surface2` segment, left-aligned; in light the segment is only slightly darker than the bar but legible.
- Toast: `surface1` panel with `shadow-md`, 2 px accent rail on the left, close "×", crisp (not mid-fade); it floats over the header text as designed (fixed top-centre).
- Rail: `surfaceSidebar` with the small spaced "NEEDS ATTENTION" header; in light it is `#f4f4f5` against the white grid, clearly separate; in dark it is the darkest region.
- No overflow or clipping at 960 px; empty grid shows `EmptyState` text centred; the `uiset` images show the real shell's seeded monitor (3 cards with the status dots, chip `workstation · Disconnected · 3`) following `ui.set` theme.
- Focus ring: not visible in these images (no focused control); checked in `s003-t002-focus-ring-dark.png` (2 px accent ring, offset 1 px, on a keyboard-focused chip).
- Light rail surface decision (TODO closed): `surfaceSidebar`, not `surface1` (see task-001 summary).
Defects found: none left; the only issue of the sprint (toast captured mid-fade) was the snapshot script and was fixed in task-002.

## Notes for later sprints
The full fixture state of the spec (cards in every status with tool calls and dimming, rail with confirm/select dialog items, a banner in place, dialogs in place) is verified progressively by the UI tasks of sprints 005–007 (each takes its own snapshots); this task covers what exists today. Platform-webview checks (`color-mix`, `@starting-style` in WebKitGTK/WKWebView) stay with sprint-009.

## Smoke against pi-studio
The `skeleton`/`empty` images use views derived from the live dev daemon (127.0.0.1:6790, mock provider) agent list; the app has no client wiring until sprint-005, so no window or live connection was run.

## Follow-ups / TODO(verify)
- Re-run `cargo xtask ci` once `crates/model/src/event.rs` (and other in-flight crates) are formatted; the web step is already green.
- Licence grant confirmation for the ported values (AGPL-3.0 declared by pi-studio's `package.json`; D20 relicensing by the holder; other git author names) — see above.
