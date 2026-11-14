import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

import { groupOffsets, planGroups, visibleGroupedRange, type GroupMetrics } from "../lib/groupLayout";
import type { Card, GroupHeader, MonitorView } from "../lib/types";
import { AgentCard } from "./AgentCard";
import styles from "./CardGrid.module.css";
import { EmptyState } from "./EmptyState";
import { WorkspaceHeader } from "./WorkspaceHeader";

/** Used where the style sheet is not loaded (tests); `--pi-card-height`, `--pi-card-min-width`, `--pi-spacing-12`. */
const FALLBACK = { cardHeight: 128, minWidth: 280, gap: 12, headerHeight: 32 };
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

/** One section of the grouped grid: its title and its cards, with spacers for the rows not drawn. */
function GroupSection({ group, showDaemon, spacers, children }: { group: GroupHeader; showDaemon: boolean; spacers?: { paddingTop: number; paddingBottom: number }; children: ReactNode }) {
  return (
    <section className={styles.group} aria-label={group.label}>
      <WorkspaceHeader group={group} showDaemon={showDaemon} />
      <div className={styles.rows} style={spacers}>
        {children}
      </div>
    </section>
  );
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
  const [metrics, setMetrics] = useState<GroupMetrics>({
    cols: 1,
    rowHeight: FALLBACK.cardHeight + FALLBACK.gap,
    headerHeight: FALLBACK.headerHeight,
    gap: FALLBACK.gap,
  });
  // Every group's card count when the grouped list is virtualised: what the scroll maths needs.
  const groupLayout = document?.groupBy === "workspace" ? (document.groups?.layout ?? null) : null;
  const layoutRef = useRef<readonly number[] | null>(null);
  const groupBy = document?.groupBy ?? "none";
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
    const flat = metricsFor(
      rowsEl.clientWidth,
      tokenPx(scrollEl, "--pi-card-height", FALLBACK.cardHeight),
      tokenPx(scrollEl, "--pi-card-min-width", FALLBACK.minWidth),
      tokenPx(scrollEl, "--pi-spacing-12", FALLBACK.gap),
    );
    const next: GroupMetrics = {
      ...flat,
      headerHeight: tokenPx(scrollEl, "--pi-group-header-height", FALLBACK.headerHeight),
      gap: tokenPx(scrollEl, "--pi-spacing-12", FALLBACK.gap),
    };
    setMetrics((old) => (old.cols === next.cols && old.rowHeight === next.rowHeight && old.headerHeight === next.headerHeight && old.gap === next.gap ? old : next));
    const layout = layoutRef.current;
    const seen = layout
      ? visibleGroupedRange(scrollEl.scrollTop, scrollEl.clientHeight, layout, groupOffsets(layout, next), next)
      : visibleRange(scrollEl.scrollTop, scrollEl.clientHeight, next);
    setRange((old) => (old.start === seen.start && old.count === seen.count ? old : seen));
  }, []);

  // The scroll maths of a grouped list reads this: set before `measure` runs, in the same commit.
  useLayoutEffect(() => {
    layoutRef.current = groupLayout;
  });
  useLayoutEffect(measure, [measure, total, virtualised, groupLayout, groupBy]);
  // A changed layout starts at the top (the shell resets its window too).
  useEffect(() => {
    if (scroller.current) {
      scroller.current.scrollTop = 0;
    }
  }, [groupBy]);
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

  const groups = document?.groups;
  if (groupBy === "workspace" && groups) {
    const showDaemon = new Set(cards.window.map((c) => c.serverId)).size > 1;
    // Virtualised: only whole rows of the sent slice per group, spacers for the rest (see groupLayout.ts).
    const layout = groups.layout;
    const plan = virtualised && layout ? planGroups(groups.window, cards.start, cards.start + cards.window.length, layout, groupOffsets(layout, metrics), metrics) : null;
    if (!virtualised || plan) {
      return (
        <section className={styles.grid} aria-label="Agents" ref={scroller} onScroll={measure}>
          <div className={styles.groups} ref={rows} style={plan ? { paddingTop: plan.padTop, paddingBottom: plan.padBottom } : undefined}>
            {groups.window.map((group, n) => {
              const drawn = plan?.groups[n];
              const from = group.start - cards.start + (drawn?.cardFrom ?? 0);
              const members = cards.window.slice(Math.max(0, from), Math.max(0, from + (drawn ? drawn.cardTo - drawn.cardFrom : group.count)));
              return (
                <GroupSection key={group.key} group={group} showDaemon={showDaemon} spacers={drawn ? { paddingTop: drawn.padTop, paddingBottom: drawn.padBottom } : undefined}>
                  {members.map((card) => (
                    <AgentCard key={`${card.serverId}/${card.agentId}`} card={card} now={now} onSelect={onSelect} showWorkspace={false} />
                  ))}
                </GroupSection>
              );
            })}
          </div>
        </section>
      );
    }
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
