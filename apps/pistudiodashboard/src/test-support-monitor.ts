// Samples for the card grid and the drill-down panel: cards in every state, the virtualised path
// and a timeline with every row kind. Built on `sampleView()` (its ui, menus and chips).

import { sampleView } from "./test-support";
import type { Card, DaemonChip, DrillItem, DrillView, GroupHeader, MenuItem, MonitorView, View } from "./lib/types";

export function card(agentId: string, title: string, status: Card["status"], more: Partial<Card> = {}): Card {
  return {
    serverId: "srv-1",
    agentId,
    daemonName: "workstation",
    title,
    model: "mock",
    cwd: "/work/app",
    workspaceLabel: "app",
    status,
    statusLabel: status,
    dimmed: status === "closed",
    stale: false,
    staleSince: null,
    attention: { pendingDialogs: 0, errored: false },
    currentTool: null,
    lastOutputLine: null,
    usage: "none",
    selected: false,
    ...more,
  };
}

function withMonitor(more: Partial<MonitorView>): View {
  const base = sampleView();
  const document = base.document;
  if (!document) {
    throw new Error("sampleView has a monitor");
  }
  return { ...base, document: { ...document, ...more } };
}

/** Cards in every state (attention, running with usage and a tool, error, idle, initializing, unknown, closed, stale). */
export function richView(now: number = Date.now()): View {
  const fresh = { tokens: 12_345, cost: 0.42, contextPercent: 38, takenAt: now - 4_000 };
  const cards = [
    card("r1", "Approve the deploy?", "idle", { attention: { pendingDialogs: 1, errored: false }, lastOutputLine: "waiting for you" }),
    card("r2", "Refactor parser", "running", { currentTool: { label: "cargo test", status: "running" }, lastOutputLine: "running 12 tests", usage: fresh }),
    card("r3", "Fix flaky test", "error", { attention: { pendingDialogs: 0, errored: true }, lastOutputLine: "mock turn failure" }),
    card("r4", "Summarise logs", "idle", { usage: { tokens: 800, cost: 0.01, contextPercent: null, takenAt: now - 3_600_000 } }),
    card("r5", "Starting up", "initializing"),
    card("r6", "From a newer daemon", "unknown"),
    card("r7", "Write docs", "closed", { daemonName: "laptop", serverId: "srv-2" }),
    card("r8", "Index the repo", "running", { daemonName: "laptop", serverId: "srv-2", dimmed: true, stale: true, staleSince: now - 600_000 }),
  ];
  return withMonitor({
    statusChips: [
      { bucket: "running", count: 2, selected: false },
      { bucket: "error", count: 1, selected: false },
      { bucket: "idle", count: 3, selected: false },
      { bucket: "initializing", count: 1, selected: false },
      { bucket: "closed", count: 1, selected: false },
    ],
    totals: { tokens: 13_145, cost: 0.43, covered: 2, of: 8, label: "2 of 8 agents" },
    cards: { total: cards.length, start: 0, virtualised: false, window: cards },
  });
}

/** `total` cards on one daemon, of which `window` are carried from index `start` (the virtualised path). */
export function bigView(total = 150, start = 0, window = 120): View {
  const statuses: Card["status"][] = ["running", "idle", "error", "closed"];
  const cards = Array.from({ length: Math.max(0, Math.min(window, total - start)) }, (_, i) => {
    const n = start + i;
    return card(`g${n}`, `Agent ${n}`, statuses[n % statuses.length] ?? "idle");
  });
  const chip: DaemonChip = { ...(sampleView().document?.daemonChips[0] as DaemonChip), agentCount: total };
  return withMonitor({
    daemonChips: [chip],
    statusChips: [],
    totals: { tokens: 0, cost: 0, covered: 0, of: total, label: `0 of ${total} agents` },
    cards: { total, start, virtualised: total > 100, window: cards },
  });
}

export interface SampleGroup {
  label: string;
  path: string | null;
  serverId?: string;
  daemonName?: string;
  /** Statuses of the group's cards, in order. */
  statuses: Card["status"][];
}

/** A grouped view (`groupBy: "workspace"`) of the given groups, laid out the way the shell sends them. */
export function groupedOf(groups: SampleGroup[], virtualised = false): View {
  const headers: GroupHeader[] = [];
  const cards: Card[] = [];
  groups.forEach((g, gi) => {
    const serverId = g.serverId ?? "srv-1";
    const daemonName = g.daemonName ?? "workstation";
    headers.push({
      key: `${serverId.length}:${serverId}${g.path ?? ""}#${gi}`,
      serverId,
      daemonName,
      label: g.label,
      path: g.path,
      count: g.statuses.length,
      start: cards.length,
      needsAttention: g.statuses.some((s) => s === "error"),
    });
    g.statuses.forEach((status, i) => {
      cards.push(card(`w${gi}-${i}`, `${g.label} agent ${i + 1}`, status, { serverId, daemonName, cwd: g.path, workspaceLabel: g.label }));
    });
  });
  const base = withMonitor({
    totals: { tokens: 0, cost: 0, covered: 0, of: cards.length, label: `0 of ${cards.length} agents` },
    cards: { total: cards.length, start: 0, virtualised, window: cards },
    groupBy: "workspace",
    groups: { total: groups.length, window: headers, ...(virtualised ? { layout: groups.map((g) => g.statuses.length) } : {}) },
  });
  return { ...base, ui: { ...base.ui, groupBy: "workspace" } };
}

/** Four workspaces on one daemon: two sharing a folder name, one with an errored agent, and agents without a workspace. */
export function groupedView(): View {
  return groupedOf([
    { label: "a/app", path: "/work/a/app", statuses: ["error", "running", "idle", "closed"] },
    { label: "b/app", path: "/work/b/app", statuses: ["running", "idle"] },
    { label: "docs", path: "/work/docs", statuses: ["idle"] },
    { label: "No workspace", path: null, statuses: ["idle", "idle"] },
  ]);
}

/** A timeline with every row kind. */
export function drillItems(): DrillItem[] {
  return [
    { type: "system", text: "Refactor the parser into two modules" },
    { type: "turn", kind: "started" },
    { type: "reasoning", text: "The tokenizer owns the cursor, so the split is clean." },
    { type: "assistant", text: "I'll start with the tokenizer.\nThen the parser." },
    { type: "tool", callId: "c1", kind: "read", label: "src/lexer.rs", status: "completed", outputTail: ["// lexer", "pub struct Lexer {", ""] },
    { type: "tool", callId: "c2", kind: "shell", label: "cargo test", status: "running", outputTail: Array.from({ length: 30 }, (_, i) => `test t${i} ... ok`) },
    { type: "tool", callId: "c3", kind: "edit", label: "src/parser.rs", status: "error", outputTail: ["error: file not found"] },
    { type: "error", message: "mock stream error" },
    { type: "turn", kind: "failed" },
    { type: "turn", kind: "completed" },
  ];
}

/** `view` with the drill open on its first card; the Agent menu then carries `agent.deselect` with its Esc shortcut. */
export function withDrill(view: View, drill: Partial<DrillView> = {}): View {
  const header = view.document?.cards.window[0] ?? null;
  const open: DrillView = {
    key: { serverId: header?.serverId ?? "srv-1", agentId: header?.agentId ?? "a1" },
    header: header && { ...header, selected: true },
    items: drillItems(),
    truncatedAtStart: false,
    state: "live",
    stale: false,
    ...drill,
  };
  const close: MenuItem = {
    id: "agent.deselect",
    label: "Close Agent Details",
    shortcut: { text: "Escape", cmd: false, ctrl: false, shift: false, alt: false, key: "escape" },
    enabled: true,
    reason: null,
    checked: null,
  };
  return {
    ...view,
    document: view.document && { ...view.document, drill: open },
    menus: view.menus.map((menu) => (menu.label === "Agent" ? { ...menu, items: [close] } : menu)),
  };
}
