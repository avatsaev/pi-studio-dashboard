// The shapes the shell sends: `control::view` in crates/shell (the `ui` object is `UiState` in
// crates/shell/src/state.rs) and `document.inspect` in crates/engine (`src/view.rs`). Rust is the
// source of truth: keep these in step with it; the frontend never builds them itself.
// `u64` ids and epoch-ms timestamps are JSON numbers, exact only below 2^53.

/** The user's choice. `system` is resolved against the OS in the webview, never in Rust. */
export type Theme = "dark" | "light" | "system";

export type AgentStatus = "initializing" | "idle" | "running" | "error" | "closed" | "unknown";

export type ConnState =
  | "idle"
  | "probing"
  | "connecting"
  | "handshaking"
  | "syncing"
  | "live"
  | "backoff"
  | "auth_failed"
  | "host_rejected"
  | "disabled";

export interface FocusItem {
  serverId: string;
  itemId: string;
}

export interface GridWindow {
  start: number;
  count: number;
}

export type Dialog =
  | { kind: "daemonForm"; mode: "add" | "edit"; entryId?: string | null }
  | { kind: "password"; entryId: string }
  | { kind: "confirmInterrupt"; serverId: string; agentId: string }
  | { kind: "removeDaemon"; entryId: string };

export interface Toast {
  id: number;
  kind: "info" | "warning" | "error";
  text: string;
}

export interface UiState {
  theme: Theme;
  /** serverIds; empty = all daemons. */
  filterDaemons: string[];
  /** Empty = all statuses. */
  filterStatuses: AgentStatus[];
  attentionRailCollapsed: boolean;
  groupBy: GroupBy;
  /** Zoom of the window's content in percent; the app applies it to the webview, nothing here scales. */
  zoom: number;
  /** Keys of the groups drawn collapsed (`GroupHeader.key`). */
  collapsedGroups: string[];
  focusItem: FocusItem | null;
  gridWindow: GridWindow;
  dialog: Dialog | null;
  diagnosticsTarget: string | null;
  toasts: Toast[];
  /** The last command's error. Empty after a success. */
  status: string;
}

/** Fixed-vocabulary failure code plus the client's bounded, secret-free text. */
export interface ConnError {
  code: string;
  message: string | null;
}

export type DegradedReason = "no_observability";

export interface DaemonChip {
  entryId: string;
  /** Absent until the daemon has said hello; a chip without one is not selectable. */
  serverId: string | null;
  name: string;
  conn: ConnState;
  /** Epoch ms of the first disconnect since the last snapshot. */
  staleSince: number | null;
  selected: boolean;
  agentCount: number;
  /** A password is stored or held in memory; the password itself never reaches the view. */
  hasPassword: boolean;
  /** The password is held in memory only (not remembered in the keychain). */
  passwordInMemoryOnly: boolean;
  /** The saved password could not be read: the prompt says the new one will not be saved. */
  keychainUnavailable: boolean;
  /** entryId of the primary entry for the same daemon (D19). */
  duplicateOf: string | null;
  lastError: ConnError | null;
  nextRetryInMs: number | null;
  hostname: string | null;
  version: string | null;
  host: string | null;
  port: number | null;
  tls: boolean;
  webUiUrl: string | null;
  autoDetected: boolean;
  /** Plain ws:// with a password to a non-loopback host (D32). */
  unencrypted: boolean;
  degraded: DegradedReason[];
}

export interface StatusChip {
  bucket: AgentStatus;
  count: number;
  selected: boolean;
}

export interface Totals {
  tokens: number;
  cost: number;
  covered: number;
  of: number;
  /** "N of M agents": drawn as given, never recomputed here. */
  label: string;
}

export type Usage =
  | "none"
  | {
      tokens: number | null;
      cost: number | null;
      contextPercent: number | null;
      /** Epoch ms; the "taken ago" text is the frontend's job. */
      takenAt: number;
    };

export interface Card {
  serverId: string;
  agentId: string;
  daemonName: string;
  title: string;
  model: string | null;
  /** The agent's workspace: its working directory without trailing separators; null without one. */
  cwd: string | null;
  /** What to show for the workspace: the folder name (plus parents when two share it), or "No workspace". */
  workspaceLabel: string;
  status: AgentStatus;
  /** The status word shown next to the dot (never colour alone). */
  statusLabel: string;
  dimmed: boolean;
  stale: boolean;
  /** Epoch ms the daemon went stale; set with `stale`. */
  staleSince: number | null;
  attention: { pendingDialogs: number; errored: boolean };
  currentTool: { label: string; status: string } | null;
  lastOutputLine: string | null;
  usage: Usage;
  selected: boolean;
}

export interface CardWindow {
  total: number;
  /** Index in the ordered list of `window[0]`. */
  start: number;
  virtualised: boolean;
  window: Card[];
}

/** How the grid is laid out (`UiState.groupBy`, saved in the config file). */
export type GroupBy = "none" | "workspace";

/** One section of the grouped grid: a workspace on one daemon. */
export interface GroupHeader {
  /** Stable and unique among the groups; the React key, and what `view.toggleGroup` takes. */
  key: string;
  /** Position among all groups. */
  index: number;
  serverId: string;
  daemonName: string;
  label: string;
  /** The full workspace path; null for the "No workspace" group. */
  path: string | null;
  /** Agents in the group (what the badge says), whether or not its cards are drawn. */
  count: number;
  /** Cards of the group in the card list: `count`, or 0 when collapsed. */
  shown: number;
  collapsed: boolean;
  /** Index of the group's first card in the whole ordered list; it can lie before `cards.start`. */
  start: number;
  needsAttention: boolean;
}

/** Present with `groupBy: "workspace"`. */
export interface GroupsView {
  total: number;
  /** The groups that touch `cards.window`. */
  window: GroupHeader[];
  /** Every group's number of cards in the list (0 when collapsed), in order; only when `cards.virtualised`. */
  layout?: number[];
}

/** One row of the drill-down timeline (`DrillItem` in crates/model/src/drill.rs). Plain text only. */
export type DrillItem =
  | { type: "assistant"; text: string }
  | { type: "reasoning"; text: string }
  | { type: "tool"; callId: string; kind: string; label: string; status: string; outputTail: string[] | null }
  | { type: "error"; message: string }
  | { type: "turn"; kind: "started" | "completed" | "failed" | "canceled" }
  | { type: "system"; text: string };

export type DrillState = "loading" | "live" | "refetching";

/** The selected agent's live timeline, shown in place of the attention rail (`view.monitor.drill`). */
export interface DrillView {
  key: { serverId: string; agentId: string };
  /** The agent's card; absent while the agent is not in the monitor state. */
  header: Card | null;
  items: DrillItem[];
  /** At most 2,000 items are kept: older ones were dropped. */
  truncatedAtStart: boolean;
  state: DrillState;
  stale: boolean;
}

export interface Diagnostics {
  entryId: string;
  name: string;
  host: string | null;
  port: number | null;
  tls: boolean;
  conn: ConnState;
  /** Epoch ms the current state was entered. */
  since: number | null;
  lastError: { kind: string; message: string | null; at: number | null } | null;
  nextRetryInMs: number | null;
  reconnectCount: number;
  connectedSince: number | null;
  messagesPerSecond: number;
  messagesTotal: number;
  /** The ten most seen, most seen first. */
  unknownTypes: { name: string; count: number }[];
  /** Every unknown type seen, summed. */
  unknownTypeCount: number;
  unknownEnumValues: number;
  malformed: number;
  binaryFramesIgnored: number;
  framesTooLarge: number;
  droppedEvents: number;
  rtt: { lastMs: number | null; averageMs: number | null };
  server: { serverId: string | null; hostname: string | null; version: string | null; featuresPresent: string[]; featuresMissing: string[] };
  duplicateOf: string | null;
  degraded: DegradedReason[];
  health: { reachable: boolean; at: number } | null;
}

/** What `document.inspect` returns. */
export interface MonitorView {
  revision: number;
  daemonChips: DaemonChip[];
  statusChips: StatusChip[];
  totals: Totals;
  cards: CardWindow;
  drill: DrillView | null;
  railCollapsed: boolean;
  groupBy: GroupBy;
  groups?: GroupsView;
  /** The open diagnostics panel's data (`ui.diagnosticsTarget`); absent when none is open, null when its daemon is gone. */
  diagnostics?: Diagnostics | null;
}

export interface Shortcut {
  /** As written in the registry, e.g. `Cmd+Shift+Z`. */
  text: string;
  /** The platform's command key: Cmd on macOS, Ctrl elsewhere. */
  cmd: boolean;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  /** The lowercased `KeyboardEvent.key` to match. */
  key: string;
}

export interface MenuItem {
  /** A command id, or `<command>:<value>` for a param-bound entry; sent back unchanged. */
  id: string;
  label: string;
  shortcut: Shortcut | null;
  enabled: boolean;
  /** Why the item is disabled. */
  reason: string | null;
  /** Not `null` for an entry that shows a check. */
  checked: boolean | null;
}

export interface Menu {
  label: string;
  items: MenuItem[];
}

export interface ShellCommandRow {
  id: string;
  label: string;
  menu: string[];
  shortcut: string | null;
  internal: boolean;
  params: string;
  enabled: boolean;
  reason: string | null;
  layer: "shell";
}

/** Everything the frontend draws: what `ui.inspect` returns. */
export interface View {
  ui: UiState;
  document: MonitorView | null;
  menus: Menu[];
  shellCommands: ShellCommandRow[];
}

export type Reply<T = unknown> = { ok: true; result: T } | { ok: false; error: string };

/** The frontend's only way to the app. Components get one as a prop, so tests pass a fake. */
export interface Backend {
  /** A control-protocol call: `engine.execute`, `engine.commands`, `ui.inspect`, `ui.set`. */
  control(method: string, params?: Record<string, unknown>): Promise<Reply>;
  /** A menu click or a shortcut: the shell collects what the command needs, then runs it. */
  invoke(command: string): Promise<Reply>;
  /** Call `draw` whenever what should be on screen has changed. Resolves to "stop listening". */
  onView(draw: (view: View) => void): Promise<() => void>;
}
