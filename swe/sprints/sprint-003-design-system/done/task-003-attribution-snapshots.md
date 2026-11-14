# Task 003 — Attribution row, icon decision, themed snapshots

- **Sprint:** sprints/sprint-003-design-system
- **Status:** done
- **Type:** docs
- **Area:** `ATTRIBUTION.md`, frontend snapshots
- **Priority:** P2
- **Estimated size:** S
- **Depends on:** task-002

## Goal
Record the provenance of the ported design values (D20), record the icon decision, and capture and look at snapshots of the current skeleton in dark and light, writing down what was verified.

## Context / why
Rule 9 requires an `ATTRIBUTION.md` row in the change that introduces ported assets; D20 requires the relicensing statement (pi-studio's sole copyright holder licenses the ported design values to this project as `MIT OR Apache-2.0`). Rule 5 requires looking at rendered images in both themes before calling UI work done; this task is the checkpoint that closes the design-system sprint with evidence.

## Scope references
- `swe/specs/architecture/design-system.md` § Provenance (the exact row), § Icons (decision: inline SVG for v1; `lucide-react` ISC row only if adopted later), § Snapshot verification, § Acceptance criteria (ATTRIBUTION item; snapshot item for what exists), § TODO(verify) (licence wording vs repo `LICENSE-MIT`/`LICENSE-APACHE`; pi-studio commit hash)
- `swe/grills/daemon-monitoring-dashboard.md` D20
- Real files: `ATTRIBUTION.md`, `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `apps/pistudiodashboard/scripts/snapshot.mjs`, `apps/pistudiodashboard/package.json` (dependencies — to confirm no icon library was added), `crates/shell/examples/view.rs`

## What to build
- **`ATTRIBUTION.md`**: add the row from the spec — Path `apps/pistudiodashboard/src/tokens.css`; Title "Pi Studio design tokens (dark and light colour, spacing, font-size ladder, radii, shadows)"; Author "avatsaev (sole copyright holder of pi-studio)"; Source "pi-studio `packages/web-client/src/theme/{colors,variants,tokens}.ts` at commit `<hash read in sprint-003/task-001>` — values only; no source files copied"; Licence "`MIT OR Apache-2.0` — relicensed to this project by the copyright holder (D20)". Verify the wording against the repo's licence files (both exist at the root; TODO(verify) closes here) and confirm `NOTICE` needs no change. If task-001 already added the row, verify it matches this text and the recorded commit.
- **Icons**: confirm the frontend adds no icon dependency (`package.json` unchanged) and that the glyphs used are inline SVG drawn for this app; state the decision in the task summary. (If a library was adopted instead, add its row — `lucide-react` / Lucide Contributors / lucide.dev / `ISC`.)
- **Snapshots**: run `npm run snapshot` in dark and in light against the current skeleton using a saved view (`--view <file>` produced from `sampleView()` data or the shell `view` example) containing: two daemon chips (one live, one backoff/stale), the empty card grid state, the right-rail area, a toast in `ui.toasts`, and the status bar; plus once for the empty monitor. Look at every image (rule 5): check contrast of text/hairlines/chips in both themes, the status bar height and segment look, the toast rail and shadow, no overflow, focus ring visible on a keyboard-focused control. Fix visible defects found (small CSS fixes allowed here; larger ones become new tasks). Record in the task summary: commands run, image names, what was checked, and any defect found/fixed.
- Note for later sprints in the task summary: the full fixture state of the spec (cards in every status, rail with confirm/select dialogs, banner) is verified progressively by the UI tasks of sprints 005–007; this task covers only what exists today.

## Out of scope
- Cards, rail items, banners in place, dialogs in place (later UI tasks each take their own snapshots).
- Platform-webview checks (WebKitGTK/WKWebView) → sprint-009.
- Adding any icon or font asset.

## Acceptance criteria
- [ ] Given `ATTRIBUTION.md`, then it contains the D20 provenance row for `tokens.css` stating `MIT OR Apache-2.0` with the real pi-studio commit hash, and the licence wording matches `LICENSE-MIT`/`LICENSE-APACHE`. (design-system AC; rule 9)
- [ ] Given `apps/pistudiodashboard/package.json` and `src/`, then no icon/font library or bundled font is present and the icon decision is recorded.
- [ ] Given `npm run snapshot` once per theme (dark, light) plus the empty monitor, then the images exist, render the skeleton from tokens with legible contrast in both themes, and the summary records that they were looked at and what was found.
- [ ] Given the full gate, then `cargo xtask ci` passes.

## Test / verification plan
- Commands: `cd apps/pistudiodashboard && npm run snapshot -- --out <name>.png --view <file>` (twice) and with `ui.set` themes via `--script` as documented in the script header; `npm run check && npm run lint && npm test`.
- Gate: `cargo xtask ci` passes.

## Notes
- Keep generated PNGs out of git unless the repo already tracks snapshot images (check `.gitignore`); the written record in the done-summary is the evidence.
- TODO(verify) (design-system.md): Brief D20 licence wording check — closed by this task; light-theme rail surface choice (`surfaceSidebar` vs `surface1`) — record the decision made in task-001 after viewing the light snapshot.
