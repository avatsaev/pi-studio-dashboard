import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import type { Card, MonitorView } from "../lib/types";
import { AgentCard } from "./AgentCard";
import styles from "./CardGrid.module.css";
import { EmptyState } from "./EmptyState";

/** Used where the style sheet is not loaded (tests); `--pi-card-height`, `--pi-card-min-width`, `--pi-spacing-12`. */
const FALLBACK = { cardHeight: 128, minWidth: 280, gap: 12 };
/** At most one `view.setGridWindow` per this many ms while scrolling; the last position always goes out. */
const REPORT_EVERY_MS = 100;

export interface Metrics {
  cols: number;
  rowHeight: number;
}

/** A px custom property of `el`, or the fallback when it is unset. */
function tokenPx(el: Element, name: string, fallback: number): number {
  const value = Number.parseFloat(getComputedStyle(el).getPropertyValue(name));
  return Number.isFinite(value) ? value : fallback;
}

/** Columns the CSS grid (`auto-fill, minmax(min, 1fr)`) lays out in `width`, and the pitch of a row. */
export function metricsFor(width: number, cardHeight: number, minWidth: number, gap: number): Metrics {
  return { cols: Math.max(1, Math.floor((width + gap) / (minWidth + gap))), rowHeight: cardHeight + gap };
}

/** The index range on screen: whole rows, one more than fit, so a scroll never shows a gap. */
export function visibleRange(scrollTop: number, viewHeight: number, { cols, rowHeight }: Metrics): { start: number; count: number } {
  const firstRow = Math.max(0, Math.floor(scrollTop / rowHeight));
  const rows = Math.max(1, Math.ceil(viewHeight / rowHeight) + 1);
  return { start: firstRow * cols, count: rows * cols };
}

function emptyState(document: MonitorView | null, onClearFilters: () => void, onAddDaemon: () => void) {
  if (!document || document.daemonChips.length === 0) {
    return <EmptyState title="No daemons yet. Add one from the Daemon menu." action={{ label: "Add daemon", onClick: onAddDaemon }} />;
  }
  const filtered = document.daemonChips.some((chip) => chip.selected) || document.statusChips.some((chip) => chip.selected);
  if (filtered) {
    return <EmptyState title="No agents match the filters." detail="The selected daemons and statuses hide every agent." action={{ label: "Clear filters", onClick: onClearFilters }} />;
  }
  return <EmptyState title="No agents on the connected daemons." />;
}

/**
 * The agent cards in the order the shell sent. Up to 100 cards all are rendered; above that
 * (`cards.virtualised`) only the slice the shell sent is, between two spacers that keep the scroll
 * height of the whole list, and the visible index range is reported through `onWindow`
 * (`view.setGridWindow`) as the user scrolls. Every card is the same height, so rows are exact.
 */
export function CardGrid({
  document,
  now,
  onSelect,
  onWindow,
  onClearFilters,
  onAddDaemon,
}: {
  document: MonitorView | null;
  now: number;
  onSelect: (card: Card) => void;
  onWindow: (window: { start: number; count: number }) => void;
  onClearFilters: () => void;
  onAddDaemon: () => void;
}) {
  const cards = document?.cards;
  const scroller = useRef<HTMLElement>(null);
  const rows = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] = useState<Metrics>({ cols: 1, rowHeight: FALLBACK.cardHeight + FALLBACK.gap });
  const [range, setRange] = useState({ start: 0, count: 0 });
  const virtualised = cards?.virtualised ?? false;
  const total = cards?.total ?? 0;

  /** Re-measure the grid and the scroll position: on mount, scroll, resize and a change of the list. */
  const measure = useCallback(() => {
    const scrollEl = scroller.current;
    const rowsEl = rows.current;
    if (!scrollEl || !rowsEl) {
      return;
    }
    const next = metricsFor(
      rowsEl.clientWidth,
      tokenPx(scrollEl, "--pi-card-height", FALLBACK.cardHeight),
      tokenPx(scrollEl, "--pi-card-min-width", FALLBACK.minWidth),
      tokenPx(scrollEl, "--pi-spacing-12", FALLBACK.gap),
    );
    setMetrics((old) => (old.cols === next.cols && old.rowHeight === next.rowHeight ? old : next));
    const seen = visibleRange(scrollEl.scrollTop, scrollEl.clientHeight, next);
    setRange((old) => (old.start === seen.start && old.count === seen.count ? old : seen));
  }, []);

  useLayoutEffect(measure, [measure, total, virtualised]);
  useEffect(() => {
    const scrollEl = scroller.current;
    if (!scrollEl || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(measure);
    observer.observe(scrollEl);
    return () => observer.disconnect();
  }, [measure]);

  // Report the visible range to the shell: now if the last report is old enough, else once at the end of the interval.
  const lastSent = useRef<{ start: number; count: number; at: number } | null>(null);
  const report = useRef(onWindow);
  useEffect(() => {
    report.current = onWindow;
  });
  useEffect(() => {
    if (!virtualised || range.count === 0) {
      lastSent.current = null;
      return;
    }
    if (lastSent.current?.start === range.start && lastSent.current.count === range.count) {
      return;
    }
    const send = () => {
      lastSent.current = { ...range, at: Date.now() };
      report.current(range);
    };
    const wait = lastSent.current ? lastSent.current.at + REPORT_EVERY_MS - Date.now() : 0;
    if (wait <= 0) {
      send();
      return;
    }
    const timer = setTimeout(send, wait);
    return () => clearTimeout(timer);
  }, [virtualised, range]);

  if (!cards || cards.total === 0) {
    return (
      <section className={styles.grid} aria-label="Agents" ref={scroller}>
        <div className={styles.empty}>{emptyState(document, onClearFilters, onAddDaemon)}</div>
      </section>
    );
  }

  // Which cards to draw: all of them, or the whole rows of the slice the shell sent.
  let shown = cards.window;
  let before = 0;
  let after = 0;
  if (virtualised) {
    const firstRow = Math.ceil(cards.start / metrics.cols);
    shown = cards.window.slice(firstRow * metrics.cols - cards.start);
    const drawnRows = Math.ceil(shown.length / metrics.cols);
    before = firstRow * metrics.rowHeight;
    after = Math.max(0, Math.ceil(cards.total / metrics.cols) - firstRow - drawnRows) * metrics.rowHeight;
  }
  return (
    <section className={styles.grid} aria-label="Agents" ref={scroller} onScroll={measure}>
      <div className={styles.rows} ref={rows} style={virtualised ? { paddingTop: before, paddingBottom: after } : undefined}>
        {shown.map((card) => (
          <AgentCard key={`${card.serverId}/${card.agentId}`} card={card} now={now} onSelect={onSelect} />
        ))}
      </div>
    </section>
  );
}
