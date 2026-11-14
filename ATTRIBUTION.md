# Attribution

Every bundled third-party asset (font, icon, image, sound, data file) gets one row here, in the
change that adds it. Assets and dependencies, Rust crates and npm packages alike, must be under a
permissive licence: `ISC`, `MIT`, `BSD-2-Clause`, `BSD-3-Clause`, `Apache-2.0`, `OFL-1.1`,
`CC0-1.0`, `CC-BY-4.0` or similar.

| Path | Title | Author | Source | Licence |
|---|---|---|---|---|
| `apps/pistudiodashboard/src-tauri/icons/` | Placeholder application icon (a check mark on a rounded square) | The project template | Generated for the template | CC0-1.0 |
| `apps/pistudiodashboard/src/tokens.css` | Pi Studio design tokens (dark and light colour, spacing, font-size ladder, radii, shadows) | avatsaev (sole copyright holder of pi-studio) | pi-studio `packages/web-client/src/theme/{colors,variants,tokens}.ts` at commit `2581365399bf166157dc1a7dfdb6ee49df6fbac3` — values only; no source files copied | `MIT OR Apache-2.0` — relicensed to this project by the copyright holder (D20) |

Replace the placeholder icon before release: `npm run tauri icon <your-1024px.png>` in
`apps/pistudiodashboard` regenerates every size. Pi Studio Dashboard bundles no fonts; the frontend uses the system's.
