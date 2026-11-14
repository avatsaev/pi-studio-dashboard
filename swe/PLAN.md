# Implementation Plan — Pi Studio Dashboard

> Derived from the specs in `swe/specs/` (brief `swe/grills/daemon-monitoring-dashboard.md`,
> decisions D1–D38) and the existing source (the av-rust-desktop scaffold). Sprints and tasks are in
> implementation order. Execute with `av-swe implement`, one sprint at a time.
> **Version:** 2 · **Updated:** 2026-11-14 (v2: sprint 011, workspace grouping, from `specs/features/agent-workspace-grouping.md`)

## Strategy

Bottom-up, keeping `cargo xtask ci` green after every task (golden rule 8):

1. **Foundation (001–002).** New core crates are built *beside* the sample checklist first
   (`protocol`, `model`, `config`, layer rules), then the sample domain is cut over in small steps:
   ingest reducers, engine/shell/CLI, `UiState`, frontend skeleton, docs. After sprint 002 the app
   runs with an empty monitor and no sample code is left.
2. **Look (003).** pi-studio tokens and primitive components come before any feature UI, so every
   later UI task is built from them and checked by snapshot in both themes.
3. **Transport (004).** The L4 client is built and tested against a scripted fake daemon and the
   vendored pi-studio fixtures (the wire contract, D22, D37) before it is wired into the app.
4. **Features in dependency order (005–008).** Daemon connections → agent monitor → attention and
   the two interventions → notifications and tray. Each sprint ends with a usable, visible slice.
5. **Hardening (009).** Load budgets (D33), a scripted pass against a real pi-studio dev daemon, docs.
6. **Stage 2 (010).** Health and logs panels over the shipped `daemonObservability` API. pi-studio
   already shipped its side of both stages, so nothing here waits on another repo.
7. **Workspace grouping (011).** Added after scoping on 2026-11-14. Depends only on sprint 006 (done)
   and touches no code that sprints 007–010 plan to change beyond `view.rs`/`CardGrid.tsx` call sites,
   so it is appended rather than renumbering the un-started sprints. Bottom-up again: L2 derivations
   and the config key, then the engine view, the shell command, and the UI last.

Pure logic (reducers, classification, ordering, bounds) lands in L1/L2 with unit and property tests;
I/O lands only in L4 and the app binary; the frontend only draws the view.

## Sprint overview
| # | Sprint | Goal | Tasks |
|---|--------|------|-------|
| 001 | `sprints/sprint-001-core-crates` | `protocol`, `model`, `config` crates + vendored fixtures + layer rules, beside the sample | 5 |
| 002 | `sprints/sprint-002-domain-cutover` | Sample domain removed; engine/shell/frontend run on `MonitorState` with an empty monitor | 5 |
| 003 | `sprints/sprint-003-design-system` | pi-studio `--pi-*` tokens, dark/light/system, primitive components | 3 |
| 004 | `sprints/sprint-004-protocol-client` | L4 client connects, authenticates, syncs and enforces the 2-mutation allowlist (fake daemon) | 5 |
| 005 | `sprints/sprint-005-daemon-connections` | Daemons are added, persisted, connected (keychain, auto-detect) and shown with diagnostics; CLI config commands | 7 |
| 006 | `sprints/sprint-006-agent-monitor` | Live card grid, filters, usage totals, drill-down | 6 |
| 007 | `sprints/sprint-007-attention-interventions` | Attention rail; answer confirm/select dialogs; interrupt; open in web UI | 5 |
| 008 | `sprints/sprint-008-notifications-tray` | OS notifications (coalesced, content-free), click-to-focus, tray, close-to-tray | 4 |
| 009 | `sprints/sprint-009-stage1-hardening` | Load budgets, real-daemon acceptance pass, stage 1 docs | 3 |
| 010 | `sprints/sprint-010-health-logs` | Stage 2: daemon health panel and live logs | 5 |
| 011 | `sprints/sprint-011-workspace-grouping` | Workspace (`cwd`) on every card; group-by-workspace grid (default, switchable and persisted), virtualised | 7 |
| 012 | `sprints/sprint-012-view-zoom` | View menu Zoom In / Out / Actual Size with shortcuts (added on request, 2026-11-14) | 1 |

**Total: 12 sprints, 56 tasks.**

## Task index

### sprints/sprint-001-core-crates
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Protocol crate: envelope, caps, lenient decoding | feature | L0 | none | architecture/protocol-client.md · `crates/protocol/`, `xtask/src/layers.rs` |
| task-002 | Vendor pi-studio fixtures + fixture harness | test | L0/testkit | task-001 | architecture/protocol-client.md · `crates/protocol/tests/fixtures/pi-studio/`, `crates/testkit/` |
| task-003 | Model crate: `MonitorState` and bounds | feature | L1 | task-001 | architecture/live-state.md · `crates/model/` |
| task-004 | Config crate: file format, recovery, atomic write | feature | L3 | none | architecture/config-and-secrets.md · `crates/config/` |
| task-005 | Layer rules: forbidden deps, explicit wasm list | chore | xtask | task-001, task-003, task-004 | architecture/app-structure.md · `xtask/src/{layers,main}.rs` |

### sprints/sprint-002-domain-cutover
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Ingest crate: connection-level reducers, derivations | feature | L2 | sprint-001/task-003 | architecture/live-state.md · `crates/ingest/` |
| task-002 | Engine cutover: `MonitorState`, `feed.ingest`, effects, sample removed | refactor | L5/L6/CLI | task-001, sprint-001/task-004 | architecture/app-structure.md, live-state.md · `crates/engine/`, `crates/shell/`, `apps/pistudiodashboard-cli/`, `crates/{doc,ops,format}` removed |
| task-003 | `UiState` and `view.*` shell commands | feature | L6 | task-002 | architecture/app-structure.md · `crates/shell/src/{state,control,menus}.rs` |
| task-004 | Frontend cutover: types, layout skeleton | feature | frontend | task-003 | features/agent-monitor.md · `apps/pistudiodashboard/src/` |
| task-005 | Docs: decision record, control protocol, workspace map | docs | docs | task-004 | architecture/app-structure.md · `docs/`, `AGENTS.md`, `README.md` |

### sprints/sprint-003-design-system
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Port pi-studio tokens, theme switching, token integrity | feature | frontend | sprint-002/task-004 | architecture/design-system.md · `src/tokens.css`, `src/tokens.test.ts` |
| task-002 | Primitive components | feature | frontend | task-001 | architecture/design-system.md, features/agent-monitor.md · `src/components/` |
| task-003 | Attribution and snapshot verification | docs | frontend | task-002 | architecture/design-system.md · `ATTRIBUTION.md` |

### sprints/sprint-004-protocol-client
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Handshake wire types and capability set | feature | L0 | sprint-001/task-002 | architecture/protocol-client.md · `crates/protocol/` |
| task-002 | Client crate: connection state machine, backoff, batching | feature | L4 | task-001, sprint-002/task-001 | architecture/protocol-client.md · `crates/client/`, `crates/testkit/` |
| task-003 | Auth, handshake errors, RTT, diagnostics counters | feature | L4 | task-002 | architecture/protocol-client.md, features/daemon-connections.md · `crates/client/` |
| task-004 | Outbound requests, correlation, mutation allowlist | feature | L0/L4 | task-003 | architecture/protocol-client.md, architecture/app-structure.md · `crates/protocol/`, `crates/client/` |
| task-005 | Sync-on-connect → `Snapshot` | feature | L0/L4 | task-004 | architecture/protocol-client.md · `crates/protocol/`, `crates/client/` |

### sprints/sprint-005-daemon-connections
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | App runtime wiring: feed pump, hooks, view coalescing | feature | app | sprint-004/task-005, sprint-002/task-002 | architecture/app-structure.md · `apps/pistudiodashboard/src-tauri/src/` |
| task-002 | `daemon.*` commands with config persistence | feature | L5 | task-001, sprint-001/task-004 | features/daemon-connections.md, architecture/config-and-secrets.md · `crates/engine/src/daemon_cmds.rs` |
| task-003 | Keychain passwords and password prompt | feature | app/L5 | task-002 | architecture/config-and-secrets.md · `src-tauri/src/services.rs`, `crates/engine/` |
| task-004 | Local daemon auto-detect and startup | feature | app | task-002 | features/daemon-connections.md, architecture/app-structure.md · `src-tauri/src/main.rs` |
| task-005 | Daemon chips, errors, badges, dialogs (UI) | feature | frontend | task-003, sprint-003/task-002 | features/daemon-connections.md · `apps/pistudiodashboard/src/components/` |
| task-006 | Diagnostics panel | feature | L6/frontend | task-005 | features/daemon-connections.md · `crates/shell/`, `src/components/` |
| task-007 | CLI offline config commands | feature | CLI/L3 | task-002 | architecture/app-structure.md § CLI scope, architecture/config-and-secrets.md · `apps/pistudiodashboard-cli/` |

### sprints/sprint-006-agent-monitor
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Agent wire types and client events | feature | L0/L4 | sprint-004/task-005 | architecture/protocol-client.md · `crates/protocol/`, `crates/client/` |
| task-002 | Agent reducers and activity summary | feature | L2 | task-001, sprint-002/task-001 | architecture/live-state.md · `crates/ingest/` |
| task-003 | Stats polling (engine `stats_tick` + `feed.tick`; client executor) and usage totals | feature | L5/L4/L2 | task-002 | architecture/protocol-client.md, live-state.md, features/agent-monitor.md · `crates/ingest/`, `crates/engine/`, `crates/client/`, `src-tauri/src/bridge.rs` |
| task-004 | Card grid, filters, virtualisation, totals (UI) | feature | L6/frontend | task-003, sprint-003/task-002 | features/agent-monitor.md · `crates/shell/`, `src/components/` |
| task-005 | Drill-down: select/deselect, timeline fetch, buffer | feature | L5/L2/L4 | task-002 | architecture/live-state.md, protocol-client.md, features/agent-monitor.md · `crates/engine/`, `crates/ingest/`, `crates/client/` |
| task-006 | Drill-down panel (UI) | feature | frontend | task-005, task-004 | features/agent-monitor.md, architecture/design-system.md · `src/components/` |

### sprints/sprint-007-attention-interventions
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Dialog wire types, classification, reducers | feature | L0/L2 | sprint-006/task-002 | architecture/live-state.md, features/attention-and-interventions.md · `crates/protocol/`, `crates/ingest/` |
| task-002 | `dialog.respond` and the mutation guard sweep | feature | L5 | task-001, sprint-005/task-001 | features/attention-and-interventions.md, architecture/app-structure.md · `crates/engine/src/agent_cmds.rs` |
| task-003 | `agent.interrupt` with confirmation | feature | L5 | task-002 | features/attention-and-interventions.md · `crates/engine/` |
| task-004 | Open in web UI | feature | L5/app | sprint-005/task-002 | features/attention-and-interventions.md · `crates/engine/`, `src-tauri/src/services.rs` |
| task-005 | Attention rail (UI) | feature | frontend | task-003, task-004, sprint-006/task-004 | features/attention-and-interventions.md, architecture/design-system.md · `src/components/` |

### sprints/sprint-008-notifications-tray
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Notification policy: toggles, coalescing, content | feature | L5 | sprint-007/task-001 | features/notifications-and-tray.md · `crates/engine/` |
| task-002 | OS notifications and click-to-focus | feature | app | task-001 | features/notifications-and-tray.md · `src-tauri/src/` |
| task-003 | Tray and close-to-tray | feature | app/L5 | task-002 | features/notifications-and-tray.md · `src-tauri/src/`, `crates/engine/` |
| task-004 | Focus highlight and settings menu states (UI) | feature | frontend | task-003, sprint-007/task-005 | features/notifications-and-tray.md · `src/components/` |

### sprints/sprint-009-stage1-hardening
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Load harness and performance budgets | test | cross-cutting | sprint-008/task-004 | architecture/live-state.md, overview (D33) · `crates/testkit/`, benches |
| task-002 | Real-daemon acceptance pass | test | cross-cutting | task-001 | all stage 1 feature specs · `../pi-studio` dev daemon |
| task-003 | Stage 1 docs | docs | docs | task-002 | overview.md · `README.md`, `docs/`, `AGENTS.md` |

### sprints/sprint-010-health-logs
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Observability wire types and client events | feature | L0/L4 | sprint-004/task-004 | features/daemon-health-and-logs.md, architecture/protocol-client.md · `crates/protocol/`, `crates/client/` |
| task-002 | Health state and `health.*` commands | feature | L1/L2/L5 | task-001 | features/daemon-health-and-logs.md · `crates/model/`, `crates/ingest/`, `crates/engine/` |
| task-003 | Logs state and `logs.*` commands | feature | L1/L2/L5 | task-002 | features/daemon-health-and-logs.md · `crates/model/`, `crates/ingest/`, `crates/engine/` |
| task-004 | Health panel (UI) | feature | frontend | task-002, sprint-006/task-006 | features/daemon-health-and-logs.md · `src/components/` |
| task-005 | Logs panel (UI) | feature | frontend | task-003, task-004 | features/daemon-health-and-logs.md · `src/components/` |

### sprints/sprint-011-workspace-grouping
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | Workspace derivations: normalise, label, group order | feature | L2 | none (sprint-006 done) | features/agent-workspace-grouping.md, architecture/live-state.md · `crates/ingest/src/derive.rs` |
| task-002 | `settings.groupBy` in the config file + engine save API | feature | L3/L5 | none | features/agent-workspace-grouping.md, architecture/config-and-secrets.md · `crates/config/src/{model,parse}.rs`, `crates/engine/src/daemon_cmds.rs` |
| task-003 | Monitor view: workspace on cards, `groupBy` param, `groups` block | feature | L5 | task-001, task-002 | features/agent-workspace-grouping.md, features/agent-monitor.md · `crates/engine/src/view.rs`, `docs/control-protocol.md` |
| task-004 | `view.setGroupBy`: UiState, View menu, `ui.set`, persistence, startup seed | feature | L6/app | task-002, task-003 | features/agent-workspace-grouping.md, architecture/app-structure.md · `crates/shell/src/{state,fields,menus,control,lib}.rs`, `src-tauri/src/owner.rs` |
| task-005 | Workspace on cards and grouped sections (UI, non-virtualised) | feature | frontend | task-004 | features/agent-workspace-grouping.md, architecture/design-system.md · `src/lib/types.ts`, `src/components/{AgentCard,CardGrid,WorkspaceHeader}.tsx`, `src/tokens.css` |
| task-006 | Virtualised grouped layout (UI) | feature | frontend | task-005 | features/agent-workspace-grouping.md, features/agent-monitor.md § Virtualisation · `src/components/CardGrid.tsx` |
| task-007 | Smoke against pi-studio, screenshots, docs | test | cross-cutting | task-006 | features/agent-workspace-grouping.md · `swe/screenshots/`, `docs/control-protocol.md` |

### sprints/sprint-012-view-zoom
| Task | Title | Type | Area | Depends on | Covers (specs + source) |
|------|-------|------|------|-----------|------------------------|
| task-001 | View zoom: commands, saved setting, native webview zoom, shortcuts | feature | L3/L5/L6/app/frontend | none | features/view-zoom.md · `crates/{config,engine,shell}`, `src-tauri/src/{main,zoom}.rs`, `src/lib/shortcuts.ts` |

## Coverage check

| Spec / module | Covered by |
|---------------|-----------|
| specs/overview.md | 001–010 (structure); 009/task-003 (docs) |
| specs/features/daemon-connections.md | 004/task-003, 005/task-002…006 |
| specs/features/agent-monitor.md | 002/task-004, 006/task-001…006; amendments by 011/task-003, 011/task-005, 011/task-006 |
| specs/features/agent-workspace-grouping.md | 011/task-001…007 |
| specs/features/view-zoom.md | 012/task-001 |
| specs/features/attention-and-interventions.md | 007/task-001…005 |
| specs/features/notifications-and-tray.md | 008/task-001…004 |
| specs/features/daemon-health-and-logs.md | 010/task-001…005 |
| specs/architecture/app-structure.md | 001/task-005, 002/task-002…005, 005/task-001, 005/task-004, 007/task-002, 011/task-004 |
| specs/architecture/protocol-client.md | 001/task-001…002, 004/task-001…005, 006/task-001, 006/task-003, 006/task-005, 010/task-001 |
| specs/architecture/live-state.md | 001/task-003, 002/task-001, 006/task-002, 006/task-003, 006/task-005, 007/task-001, 009/task-001, 011/task-001 |
| specs/architecture/config-and-secrets.md | 001/task-004, 005/task-002, 005/task-003, 005/task-007, 011/task-002 |
| specs/architecture/design-system.md | 003/task-001…003, every UI task (snapshots), 011/task-005 (`--pi-group-header-height`) |
| Sample domain (`crates/doc`, `ops`, `format`, `item_cmds`, `file_cmds`, `edit_cmds`, `components/Items*`) | removed by 002/task-002, 002/task-004 |
| `xtask` layers / wasm | 001/task-005 |
| Brief acceptance 1–14 (stage 1) | feature tasks above; end-to-end in 009/task-002 |
| Brief acceptance 15–17 (stage 2) | 010/task-001…005 (17 is pi-studio's, already shipped) |

No spec is uncovered.

## Open questions — TODO(verify)
- [ ] Licence checks (golden rule 9) for `tokio-tungstenite`/`rustls`/`keyring`/`tauri-plugin-notification`/tray feature — resolved inside the tasks that add each crate (004/task-002, 005/task-003, 008/task-002, 008/task-003).
- [ ] Workspace grouping (spec `agent-workspace-grouping.md`): `groupBy` is persisted and defaults to `workspace` (user decision, 2026-11-14; done in sprint 011); No-workspace group with a pending dialog sorts last (planned: yes, 011/task-001); no shortcut for `view.setGroupBy`; whether a daemon can change an agent's `cwd` (no shipped message carries it).
- [ ] Plan-time finding: `theme` and `attentionRailCollapsed` are in the config file but nothing persists or seeds them yet; 011/task-002/004 build that path for `groupBy` only. Decide whether 008/task-004 should reuse it for theme and the rail flag.
- [ ] Proposed numeric caps (50 daemon entries, 64-char names, 5,000 log lines, 2 KB dialog strings, 100 options, 15 s answer timeout) — confirm before 005/task-002 and 007/task-002.
- [ ] Dismiss for dialogs (`cancelled: true`) is not planned; add a task if wanted.
- [ ] Whether the dialog title may appear on lock-screen notifications (008/task-001 defaults to title ≤ 80 chars).
- [ ] macOS: first manual run of tray/notifications (not automatable here, D34).
- [ ] (brief, non-blocking) Today's workflow and success measure — owner: avatsaev.
- [ ] `hello.capabilities`: the spec sends `{}`; pi-studio's `hello` fixture shows `{reasoning_merge_enum:true}`. 004/task-001 follows the spec; revisit if reasoning deltas render badly.
- [x] Resolved: pi-studio's mock provider raises dialogs on demand (`run "#ui confirm"` / `"#ui select"`); see `swe/notes/smoke-harness.md`.

Plan-time decision recorded in the brief: **D39** — the engine is the only scheduler of stats and drill-down timeline fetches; the client executes, dedupes and pages.

Plan-review notes: 002/task-002 (engine cutover) is the largest task — it deletes three crates and rewrites `Session` in one step because the gate must stay green; split it during `implement` if it grows past one sitting. `swe/notes/plan-outline.md` is the design input the task files were written from.

**Smoke harness (user decision):** smoke tests run against the sibling `../pi-studio` dev daemon (mock provider, `127.0.0.1:6790`) driven by its CLI; unit/contract tests keep using the vendored fixtures and the in-repo fake daemon. Commands and rules: `swe/notes/smoke-harness.md`. Every task that touches protocol, client, engine, or app records a "Smoke against pi-studio" section in its summary.
