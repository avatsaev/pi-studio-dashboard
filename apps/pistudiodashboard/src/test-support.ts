// Shared by the component tests: a view like the shell sends, and a backend that records what
// the frontend asked for instead of running it.

import { readFileSync } from "node:fs";

import type { Backend, Card, DaemonChip, MenuItem, MonitorView, Reply, View } from "./lib/types";

function item(id: string, label: string, more: Partial<MenuItem> = {}): MenuItem {
  return { id, label, shortcut: null, enabled: true, reason: null, checked: null, ...more };
}

const defaults: View["ui"] = {
  theme: "dark",
  filterDaemons: [],
  filterStatuses: [],
  attentionRailCollapsed: false,
  groupBy: "none",
  zoom: 100,
  focusItem: null,
  gridWindow: { start: 0, count: 100 },
  dialog: null,
  diagnosticsTarget: null,
  toasts: [],
  status: "",
};

function menus(theme: View["ui"]["theme"]): View["menus"] {
  const themeItem = (value: string, label: string) => item(`view.setTheme:${value}`, label, { checked: theme === value });
  return [
    { label: "Daemon", items: [] },
    { label: "Agent", items: [] },
    {
      label: "View",
      items: [
        themeItem("dark", "Theme: Dark"),
        themeItem("light", "Theme: Light"),
        themeItem("system", "Theme: System"),
        item("view.setGroupBy:none", "Group: None", { checked: true }),
        item("view.setGroupBy:workspace", "Group: By Workspace", { checked: false }),
        item("view.zoomIn", "Zoom In", { shortcut: { text: "Cmd+=", cmd: true, ctrl: false, shift: false, alt: false, key: "=" } }),
        item("view.zoomOut", "Zoom Out", { shortcut: { text: "Cmd+-", cmd: true, ctrl: false, shift: false, alt: false, key: "-" } }),
        item("view.zoomReset", "Actual Size", { shortcut: { text: "Cmd+0", cmd: true, ctrl: false, shift: false, alt: false, key: "0" }, enabled: false, reason: "already at 100%" }),
        item("view.toggleAttentionRail", "Toggle Attention Rail", {
          shortcut: { text: "Cmd+Shift+A", cmd: true, ctrl: false, shift: true, alt: false, key: "a" },
        }),
      ],
    },
  ];
}

const shellCommands: View["shellCommands"] = [
  { id: "view.setTheme", label: "Theme", menu: ["View"], shortcut: null, internal: false, params: "{}", enabled: true, reason: null, layer: "shell" },
  { id: "view.toggleAttentionRail", label: "Toggle Attention Rail", menu: ["View"], shortcut: "Cmd+Shift+A", internal: false, params: "{}", enabled: true, reason: null, layer: "shell" },
];

function card(agentId: string, title: string, status: Card["status"], more: Partial<Card> = {}): Card {
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

export function chip(entryId: string, name: string, more: Partial<DaemonChip> = {}): DaemonChip {
  return {
    entryId,
    serverId: entryId,
    name,
    conn: "live",
    staleSince: null,
    selected: false,
    agentCount: 0,
    hasPassword: false,
    passwordInMemoryOnly: false,
    keychainUnavailable: false,
    duplicateOf: null,
    lastError: null,
    nextRetryInMs: null,
    hostname: null,
    version: null,
    host: "127.0.0.1",
    port: 6790,
    tls: false,
    webUiUrl: null,
    autoDetected: false,
    unencrypted: false,
    degraded: [],
    ...more,
  };
}

function monitor(more: Partial<MonitorView>): MonitorView {
  return {
    revision: 1,
    daemonChips: [],
    statusChips: [],
    totals: { tokens: 0, cost: 0, covered: 0, of: 0, label: "0 of 0 agents" },
    cards: { total: 0, start: 0, virtualised: false, window: [] },
    drill: null,
    railCollapsed: false,
    groupBy: "none",
    ...more,
  };
}

/** A monitor with no daemons configured. */
export function emptyView(): View {
  return { ui: { ...defaults }, document: monitor({}), menus: menus("dark"), shellCommands };
}

/** Two daemons (one live, one in backoff), four agents including an attention card. */
export function sampleView(): View {
  const cards = [
    card("a1", "Refactor parser", "running", { currentTool: { label: "cargo test", status: "running" }, lastOutputLine: "running 12 tests" }),
    card("a2", "Answer the dialog", "idle", { attention: { pendingDialogs: 1, errored: false } }),
    card("a3", "Fix flaky test", "error", { attention: { pendingDialogs: 0, errored: true } }),
    card("a4", "Write docs", "closed", { daemonName: "laptop", serverId: "srv-2" }),
  ];
  return {
    ui: { ...defaults },
    document: monitor({
      daemonChips: [
        chip("e1", "workstation", { agentCount: 3 }),
        chip("e2", "laptop", { conn: "backoff", staleSince: 1_700_000_000_000, agentCount: 1 }),
      ],
      statusChips: [
        { bucket: "running", count: 1, selected: false },
        { bucket: "idle", count: 1, selected: false },
        { bucket: "error", count: 1, selected: false },
        { bucket: "closed", count: 1, selected: false },
      ],
      totals: { tokens: 0, cost: 0, covered: 0, of: 4, label: "0 of 4 agents" },
      cards: { total: 4, start: 0, virtualised: false, window: cards },
    }),
    menus: menus("dark"),
    shellCommands,
  };
}

export type Call = ["control", string, Record<string, unknown> | undefined] | ["invoke", string];

export function fakeBackend(view: View | null, answer: (method: string, params?: Record<string, unknown>) => Reply = () => ({ ok: true, result: null })) {
  const calls: Call[] = [];
  let draw: (view: View) => void = () => {};
  const backend: Backend = {
    control: async (method, params) => {
      if (method === "ui.inspect") {
        return view ? { ok: true, result: view } : { ok: false, error: "no view" };
      }
      calls.push(["control", method, params]);
      return answer(method, params);
    },
    invoke: async (command) => {
      calls.push(["invoke", command]);
      return { ok: true, result: null };
    },
    onView: async (listener) => {
      draw = listener;
      return () => {};
    },
  };
  return { backend, calls, emit: (next: View) => draw(next) };
}

/** The `@media (prefers-reduced-motion: reduce)` blocks of a style sheet next to `src`, for tests of "no motion". */
export function reducedMotionCss(file: string): string {
  const css = readFileSync(new URL(file, import.meta.url), "utf8");
  return [...css.matchAll(/@media \(prefers-reduced-motion: reduce\)\s*\{((?:[^{}]|\{[^{}]*\})*)\}/g)].map((block) => block[1]).join("\n");
}
