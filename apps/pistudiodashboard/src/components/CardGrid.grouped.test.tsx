import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { groupOffsets, visibleGroupedRange } from "../lib/groupLayout";
import type { MonitorView } from "../lib/types";
import { groupedOf, groupedView, type SampleGroup } from "../test-support-monitor";
import { CardGrid } from "./CardGrid";

const noop = () => {};
function grid(document: MonitorView | null) {
  return <CardGrid document={document} now={Date.now()} onSelect={noop} onWindow={noop} onClearFilters={noop} onAddDaemon={noop} />;
}

describe("CardGrid grouped by workspace", () => {
  it("draws one labelled section per group, in the order sent, each with its own cards", () => {
    render(grid(groupedView().document));
    const sections = within(screen.getByRole("region", { name: "Agents" })).getAllByRole("region");
    expect(sections.map((s) => s.getAttribute("aria-label"))).toEqual(["a/app", "b/app", "docs", "No workspace"]);
    expect(sections.map((s) => within(s).getAllByRole("button").length)).toEqual([4, 2, 1, 2]);
    expect(within(sections[1] as HTMLElement).getByRole("button", { name: /b\/app agent 1/ })).toBeTruthy();
  });

  it("shows each group's count, the attention warning and the full path", () => {
    render(grid(groupedView().document));
    const first = screen.getByRole("region", { name: "a/app" });
    expect(within(first).getByText("4")).toBeTruthy();
    expect(within(first).getByText("needs attention")).toBeTruthy();
    expect(within(first).getByText("/work/a/app")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "docs" })).queryByText("needs attention")).toBeNull();
  });

  it("leaves the workspace off the cards (the header says it)", () => {
    render(grid(groupedView().document));
    const section = screen.getByRole("region", { name: "b/app" });
    const card = within(section).getAllByRole("button")[0] as HTMLElement;
    expect(card.querySelector('[class*="workspace"]')).toBeNull();
  });

  it("names the daemon in the headers only when cards of several daemons are shown", () => {
    const headerText = () => screen.getAllByRole("region", { name: "app" }).map((section) => section.firstElementChild?.textContent ?? "");
    const one = groupedOf([{ label: "app", path: "/w/app", statuses: ["idle"] }]);
    const { rerender } = render(grid(one.document));
    expect(headerText().every((t) => !t.includes("workstation"))).toBe(true);
    const two = groupedOf([
      { label: "app", path: "/w/app", statuses: ["idle"] },
      { label: "app", path: "/w/app", serverId: "srv-2", daemonName: "laptop", statuses: ["idle"] },
    ]);
    rerender(grid(two.document));
    expect(headerText().map((t) => (t.includes("workstation") ? "workstation" : t.includes("laptop") ? "laptop" : ""))).toEqual(["workstation", "laptop"]);
  });

  it("keeps the same group sections when only a card's text changes", () => {
    const first = groupedView();
    const { rerender } = render(grid(first.document));
    const before = screen.getAllByRole("region", { name: /./ }).map((r) => r.getAttribute("aria-label"));
    const next = groupedView();
    const doc = next.document as MonitorView;
    const card = doc.cards.window[0];
    if (card) {
      card.lastOutputLine = "new line";
    }
    rerender(grid(doc));
    expect(screen.getAllByRole("region", { name: /./ }).map((r) => r.getAttribute("aria-label"))).toEqual(before);
    expect(screen.getByText("new line")).toBeTruthy();
  });
});

/** jsdom has no layout: give every element a size and a scroll position we control. */
const screenSize = { width: 1000, height: 600, top: 0 };
const COLS = 3;
const PITCH = 140;
const HEADER = 32;
const GAP = 12;

beforeEach(() => {
  screenSize.width = 1000;
  screenSize.top = 0;
  vi.spyOn(Element.prototype, "clientWidth", "get").mockImplementation(() => screenSize.width);
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(() => screenSize.height);
  vi.spyOn(Element.prototype, "scrollTop", "get").mockImplementation(() => screenSize.top);
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

/** 500 agents in 200 workspaces (2 or 3 each). */
const GROUPS: SampleGroup[] = Array.from({ length: 200 }, (_, i) => ({
  label: `ws${String(i).padStart(3, "0")}`,
  path: `/w/ws${String(i).padStart(3, "0")}`,
  statuses: Array.from({ length: i < 100 ? 3 : 2 }, () => "idle" as const),
}));
const LAYOUT = GROUPS.map((g) => g.statuses.length);
const TOTAL = LAYOUT.reduce((a, b) => a + b, 0);

/** What the shell would send for a reported window: the slice plus 20 cards of overscan, at most 200. */
function shellView(start: number, count: number): MonitorView {
  const full = groupedOf(GROUPS, true).document as MonitorView;
  const from = Math.max(0, Math.min(start, TOTAL) - 20);
  const to = Math.min(TOTAL, Math.min(start, TOTAL) + count + 20, from + 200);
  const headers = (full.groups?.window ?? []).filter((g) => g.start < to && g.start + g.count > from);
  return { ...full, cards: { ...full.cards, start: from, window: full.cards.window.slice(from, to) }, groups: { total: GROUPS.length, window: headers, layout: LAYOUT } };
}

/** The height the drawn DOM adds up to, from the spacers it carries and the rows it holds. */
function drawnTotal(): number {
  const groups = document.querySelector('[class*="groups"]') as HTMLElement;
  const px = (el: HTMLElement, side: "paddingTop" | "paddingBottom") => Number.parseFloat(el.style[side] || "0");
  let sum = px(groups, "paddingTop") + px(groups, "paddingBottom");
  const sections = Array.from(groups.children) as HTMLElement[];
  sections.forEach((section, i) => {
    const grid = section.children[1] as HTMLElement;
    const cards = grid.querySelectorAll("button").length;
    const rows = Math.ceil(cards / COLS);
    sum += HEADER + GAP + px(grid, "paddingTop") + px(grid, "paddingBottom") + (rows > 0 ? rows * PITCH - GAP : 0);
    if (i > 0) {
      sum += GAP;
    }
  });
  return sum;
}

describe("CardGrid grouped by workspace, virtualised", () => {
  const offsets = groupOffsets(LAYOUT, { cols: COLS, rowHeight: PITCH, headerHeight: HEADER, gap: GAP });

  it("keeps the whole list's height whatever slice it holds: spacers plus drawn sections add up exactly", () => {
    for (const start of [0, 3, 250, 251, 252, 400, 495, 499]) {
      const { unmount } = render(
        <CardGrid document={shellView(start, 40)} now={Date.now()} onSelect={noop} onWindow={noop} onClearFilters={noop} onAddDaemon={noop} />,
      );
      expect(drawnTotal(), `slice at ${start}`).toBe(offsets.total);
      unmount();
    }
  });

  it("draws only sections and cards of the slice, each card once and in order", () => {
    render(<CardGrid document={shellView(250, 40)} now={Date.now()} onSelect={noop} onWindow={noop} onClearFilters={noop} onAddDaemon={noop} />);
    const ids = screen.getAllByRole("button").map((b) => b.getAttribute("aria-pressed") && (b.querySelector('[class*="title"]')?.textContent ?? ""));
    expect(ids.length).toBeGreaterThan(0);
    expect(ids.length).toBeLessThanOrEqual(200);
    expect(new Set(ids).size).toBe(ids.length);
    const labels = screen.getAllByRole("region", { name: /^ws/ }).map((r) => r.getAttribute("aria-label"));
    expect(labels).toEqual([...labels].sort((a, b) => String(a).localeCompare(String(b))));
  });

  it("reports the visible card range of the grouped layout through the throttled window callback", () => {
    vi.useFakeTimers();
    const onWindow = vi.fn<(w: { start: number; count: number }) => void>();
    render(<CardGrid document={shellView(0, 40)} now={Date.now()} onSelect={noop} onWindow={onWindow} onClearFilters={noop} onAddDaemon={noop} />);
    screenSize.top = 20_000;
    act(() => {
      fireEvent.scroll(screen.getByRole("region", { name: "Agents" }));
      vi.advanceTimersByTime(200);
    });
    const metrics = { cols: COLS, rowHeight: PITCH, headerHeight: HEADER, gap: GAP };
    const expected = visibleGroupedRange(20_000, screenSize.height, LAYOUT, offsets, metrics);
    expect(onWindow).toHaveBeenLastCalledWith(expected);
    expect(expected.start).toBeGreaterThan(100);
  });

  it("recomputes the columns, and so the range, when the width changes", () => {
    vi.useFakeTimers();
    const onWindow = vi.fn<(w: { start: number; count: number }) => void>();
    render(<CardGrid document={shellView(0, 40)} now={Date.now()} onSelect={noop} onWindow={onWindow} onClearFilters={noop} onAddDaemon={noop} />);
    screenSize.top = 5_000;
    screenSize.width = 1700;
    act(() => {
      fireEvent.scroll(screen.getByRole("region", { name: "Agents" }));
      vi.advanceTimersByTime(200);
    });
    const wide = { cols: 5, rowHeight: PITCH, headerHeight: HEADER, gap: GAP };
    expect(onWindow).toHaveBeenLastCalledWith(visibleGroupedRange(5_000, screenSize.height, LAYOUT, groupOffsets(LAYOUT, wide), wide));
  });

  it("goes back to the top when the layout changes", () => {
    const { rerender } = render(<CardGrid document={groupedView().document} now={Date.now()} onSelect={noop} onWindow={noop} onClearFilters={noop} onAddDaemon={noop} />);
    const region = screen.getByRole("region", { name: "Agents" });
    let top = 300;
    Object.defineProperty(region, "scrollTop", { configurable: true, get: () => top, set: (v: number) => {
        top = v;
      },
    });
    const flat = { ...(groupedView().document as MonitorView), groupBy: "none" as const, groups: undefined };
    rerender(<CardGrid document={flat} now={Date.now()} onSelect={noop} onWindow={noop} onClearFilters={noop} onAddDaemon={noop} />);
    expect(top).toBe(0);
  });
});
