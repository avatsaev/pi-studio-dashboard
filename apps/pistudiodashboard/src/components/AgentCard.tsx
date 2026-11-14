import { clip, compactNumber, formatClock, formatCost, USAGE_FRESH_MS } from "../lib/format";
import type { Card, Usage } from "../lib/types";
import styles from "./AgentCard.module.css";
import { StatusBadge } from "./StatusBadge";
import { StatusDot } from "./StatusDot";

/** The usage line: "—" without a live session, a muted "as of" when the snapshot is old (D16). */
export function usageParts(usage: Usage, now: number): { tokens: string; cost: string; context: string | null; asOf: string | null } {
  if (usage === "none") {
    return { tokens: "—", cost: "—", context: null, asOf: null };
  }
  return {
    tokens: usage.tokens === null ? "—" : compactNumber(usage.tokens),
    cost: usage.cost === null ? "—" : formatCost(usage.cost),
    context: usage.contextPercent === null ? null : `${Math.round(usage.contextPercent)}%`,
    asOf: now - usage.takenAt > USAGE_FRESH_MS ? formatClock(usage.takenAt, now) : null,
  };
}

/**
 * One agent: dot, title, daemon, model, current tool, last output line, usage. Everything is plain
 * text from the daemon (never markup), cut to one line by CSS and clipped here as a second guard.
 * A click selects the agent; the card only draws what the shell says.
 */
export function AgentCard({ card, now, onSelect, showWorkspace = true }: { card: Card; now: number; onSelect: (card: Card) => void; showWorkspace?: boolean }) {
  const attention = card.attention.pendingDialogs > 0 ? "dialog" : card.attention.errored ? "error" : null;
  const usage = usageParts(card.usage, now);
  const state = attention === "dialog" ? styles.waiting : card.status === "running" ? styles.running : attention === "error" || card.status === "error" ? styles.failed : "";
  const classes = [styles.card, state, card.dimmed ? styles.dimmed : "", card.selected ? styles.selected : ""].filter(Boolean).join(" ");
  const title = clip(card.title);
  return (
    <button type="button" className={classes} aria-pressed={card.selected} onClick={() => onSelect(card)}>
      <span className={styles.head}>
        <StatusDot status={card.status} attention={attention} />
        <span className={styles.title} title={title}>
          {title}
        </span>
        {card.attention.pendingDialogs > 0 && (
          <StatusBadge tone="warning" title={`${card.attention.pendingDialogs} request(s) waiting for a response`}>
            {card.attention.pendingDialogs} pending
          </StatusBadge>
        )}
        <span className={styles.status}>{clip(card.statusLabel, 24)}</span>
      </span>
      <span className={styles.meta}>
        <span className={styles.daemon}>{clip(card.daemonName, 80)}</span>
        {showWorkspace && (
          <span className={styles.workspace} title={card.cwd ?? card.workspaceLabel}>
            {clip(card.workspaceLabel, 80)}
          </span>
        )}
        {card.model && <span className={styles.model}>{clip(card.model, 80)}</span>}
        {card.stale && <span className={styles.stale}>{card.staleSince === null ? "stale" : `stale since ${formatClock(card.staleSince, now)}`}</span>}
      </span>
      <span className={styles.tool}>
        {card.currentTool && (
          <>
            <span className={styles.toolName}>{clip(card.currentTool.label, 200)}</span>
            <span className={styles.toolStatus}>{clip(card.currentTool.status, 24)}</span>
          </>
        )}
      </span>
      <span className={styles.line}>{card.lastOutputLine ? clip(card.lastOutputLine, 300) : ""}</span>
      <span className={styles.usage}>
        <span>{usage.tokens} tokens</span>
        <span>cost {usage.cost}</span>
        {usage.context && <span>{usage.context} context</span>}
        {usage.asOf && <span className={styles.asOf}>as of {usage.asOf}</span>}
      </span>
    </button>
  );
}
