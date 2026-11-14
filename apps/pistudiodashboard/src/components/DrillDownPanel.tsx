import { useLayoutEffect, useRef, useState } from "react";

import { clip, formatClock } from "../lib/format";
import type { DrillView } from "../lib/types";
import { usageParts } from "./AgentCard";
import { Banner } from "./Banner";
import styles from "./DrillDownPanel.module.css";
import { DrillRow } from "./DrillRows";
import { CloseIcon } from "./icons";
import { StatusDot } from "./StatusDot";

/** Within this many px of the end counts as "at the bottom": the panel keeps following. */
export const FOLLOW_SLACK = 24;

export function isAtBottom(el: { scrollHeight: number; scrollTop: number; clientHeight: number }): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= FOLLOW_SLACK;
}

function Header({ drill, now, onClose }: { drill: DrillView; now: number; onClose: () => void }) {
  const card = drill.header;
  const attention = card && card.attention.pendingDialogs > 0 ? "dialog" : card?.attention.errored ? "error" : null;
  const usage = card ? usageParts(card.usage, now) : null;
  return (
    <header className={styles.header}>
      <div className={styles.titleRow}>
        {card && <StatusDot status={card.status} attention={attention} />}
        <h2 className={styles.title} title={clip(card?.title ?? drill.key.agentId)}>
          {clip(card?.title ?? drill.key.agentId)}
        </h2>
        {card && <span className={styles.status}>{clip(card.statusLabel, 24)}</span>}
        <button type="button" className={styles.close} aria-label="Close agent details" title="Close (Esc)" onClick={onClose}>
          <CloseIcon />
        </button>
      </div>
      {card && (
        <p className={styles.meta}>
          {[clip(card.daemonName, 80), card.model ? clip(card.model, 80) : null].filter(Boolean).join(" · ")}
          {card.stale && <span className={styles.stale}>{card.staleSince === null ? " · stale" : ` · stale since ${formatClock(card.staleSince, now)}`}</span>}
        </p>
      )}
      {usage && (
        <p className={styles.usage}>
          {usage.tokens} tokens · cost {usage.cost}
          {usage.context && ` · ${usage.context} context`}
          {usage.asOf && <span className={styles.asOf}> · as of {usage.asOf}</span>}
        </p>
      )}
      {/* Reserved for the Interrupt / Open-in-web-UI buttons and this agent's dialogs (sprint-007). Empty: no dead control. */}
      <div className={styles.slot} data-slot="actions" />
      <div className={styles.slot} data-slot="dialogs" />
    </header>
  );
}

/** The timeline: newest at the bottom, following the end until the user scrolls up. */
function Body({ drill }: { drill: DrillView }) {
  const scroller = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const [scrolledUp, setScrolledUp] = useState(false);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && following.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [drill.items, drill.state]);

  const onScroll = () => {
    const el = scroller.current;
    if (el) {
      following.current = isAtBottom(el);
      setScrolledUp(!following.current);
    }
  };
  const jump = () => {
    const el = scroller.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
    following.current = true;
    setScrolledUp(false);
  };

  return (
    <div className={styles.bodyWrap}>
      <div className={styles.body} ref={scroller} onScroll={onScroll} data-testid="drill-body">
        {drill.state === "refetching" && (
          <output className={styles.reloading}>Reloading…</output>
        )}
        {drill.truncatedAtStart && <p className={styles.earlier}>earlier output not shown</p>}
        {drill.state === "loading" ? (
          <output className={styles.loading} aria-label="Loading timeline">
            <span className={styles.bar} />
            <span className={styles.bar} />
            <span className={styles.bar} />
            <span className={styles.loadingText}>Loading output…</span>
          </output>
        ) : drill.items.length === 0 ? (
          <p className={styles.earlier}>No output yet.</p>
        ) : (
          drill.items.map((item, index) => <DrillRow key={index} item={item} />)
        )}
      </div>
      {scrolledUp && (
        <button type="button" className={styles.jump} onClick={jump}>
          Jump to latest
        </button>
      )}
    </div>
  );
}

/**
 * The selected agent's current turn, in place of the attention rail: header (the card repeated, a
 * close button, empty slots for sprint-007's controls), a stale banner, and the plain-text rows.
 * Esc closes it through the shortcut table (`agent.deselect`), not a key handler here.
 */
export function DrillDownPanel({ drill, now, onClose }: { drill: DrillView; now: number; onClose: () => void }) {
  return (
    <aside className={styles.panel} aria-label="Agent details">
      <Header drill={drill} now={now} onClose={onClose} />
      {drill.stale && <Banner tone="warning">daemon disconnected — showing last known output</Banner>}
      <Body key={`${drill.key.serverId}/${drill.key.agentId}`} drill={drill} />
    </aside>
  );
}
