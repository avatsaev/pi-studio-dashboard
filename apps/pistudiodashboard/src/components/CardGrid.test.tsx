import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { emptyView, sampleView } from "../test-support";
import { bigView, richView } from "../test-support-monitor";
import { CardGrid, metricsFor, visibleRange } from "./CardGrid";

const NOW = Date.now();

/** jsdom has no layout: give every element a size and a scroll position we control. */
const layout = { width: 1000, height: 600, top: 0 };

beforeEach(() => {
  layout.top = 0;
  vi.spyOn(Element.prototype, "clientWidth", "get").mockImplementation(() => layout.width);
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(() => layout.height);
  vi.spyOn(Element.prototype, "scrollTop", "get").mockImplementation(() => layout.top);
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

const noop = () => {};
function grid(document: Parameters<typeof CardGrid>[0]["document"], more: Partial<Parameters<typeof CardGrid>[0]> = {}) {
  return <CardGrid document={document} now={NOW} onSelect={noop} onWindow={noop} onClearFilters={noop} onAddDaemon={noop} {...more} />;
}

describe("window maths", () => {
  it("lays out the columns the CSS grid would and one fixed row pitch", () => {
    expect(metricsFor(1000, 128, 280, 12)).toEqual({ cols: 3, rowHeight: 140 });
    expect(metricsFor(100, 128, 280, 12)).toEqual({ cols: 1, rowHeight: 140 });
    expect(metricsFor(0, 128, 280, 12).cols).toBe(1);
  });

  it("reports whole rows from the first visible one, plus one more", () => {
    const m = { cols: 3, rowHeight: 140 };
    expect(visibleRange(0, 600, m)).toEqual({ start: 0, count: 18 });
    expect(visibleRange(1400, 600, m)).toEqual({ start: 30, count: 18 });
    expect(visibleRange(1450, 600, m)).toEqual({ start: 30, count: 18 });
  });
});

describe("CardGrid", () => {
  it("draws every card up to 100 and reports no window", () => {
    const onWindow = vi.fn<(w: { start: number; count: number }) => void>();
    render(grid(bigView(100, 0, 100).document, { onWindow }));
    expect(within(screen.getByRole("region", { name: "Agents" })).getAllByRole("button")).toHaveLength(100);
    expect(onWindow).not.toHaveBeenCalled();
  });

  it("draws cards in the order the shell sent, keeping each card's place when only text changes", () => {
    const first = richView(NOW);
    const { rerender } = render(grid(first.document));
    const order = () => screen.getAllByRole("button").map((b) => b.querySelector('[class*="title"]')?.textContent);
    const before = order();
    const next = richView(NOW);
    for (const c of next.document?.cards.window ?? []) {
      c.lastOutputLine = "streaming more text";
    }
    rerender(grid(next.document));
    expect(order()).toEqual(before);
    expect(before.length).toBe(8);
  });

  it("reports the visible range of a virtualised list, throttled, and the last position always goes out", () => {
    vi.useFakeTimers();
    const onWindow = vi.fn<(w: { start: number; count: number }) => void>();
    render(grid(bigView(500, 0, 60).document, { onWindow }));
    expect(onWindow).toHaveBeenCalledTimes(1);
    expect(onWindow).toHaveBeenLastCalledWith({ start: 0, count: 18 });
    const region = screen.getByRole("region", { name: "Agents" });
    for (const top of [1400, 2800, 4200]) {
      layout.top = top;
      fireEvent.scroll(region);
    }
    expect(onWindow).toHaveBeenCalledTimes(1);
    act(() => {
      vi.advanceTimersByTime(150);
    });
    expect(onWindow).toHaveBeenCalledTimes(2);
    expect(onWindow).toHaveBeenLastCalledWith({ start: 90, count: 18 });
  });

  it("scrolling reaches every one of 500 cards: the shell's window follows the reported range, as the backend slices it", () => {
    vi.useFakeTimers();
    // What `view.monitor` does: the reported range, ±20 overscan, at most 200 cards.
    function Harness() {
      const [document, setDocument] = useState(bigView(500, 0, 120).document);
      return grid(document, {
        onWindow: ({ start, count }) => setDocument(bigView(500, Math.max(0, start - 20), Math.min(200, count + 40)).document),
      });
    }
    render(<Harness />);
    const region = screen.getByRole("region", { name: "Agents" });
    const seen = new Set<number>();
    const draw = () => {
      for (const b of screen.getAllByRole("button")) {
        const n = /Agent (\d+)/.exec(b.querySelector('[class*="title"]')?.textContent ?? "")?.[1];
        if (n !== undefined) {
          seen.add(Number(n));
        }
      }
    };
    // 500 cards, 3 per row: 167 rows of 140 px. Scroll down a screen (600 px) at a time.
    for (let top = 0; top <= 167 * 140; top += 600) {
      layout.top = top;
      fireEvent.scroll(region);
      act(() => {
        vi.advanceTimersByTime(150);
      });
      draw();
    }
    expect(seen.size).toBe(500);
  });

  it("keeps the scroll height of the whole list with spacers and draws only whole rows of the slice", () => {
    // 3 columns, pitch 140; the shell's slice starts at card 31, so the first whole row is card 33.
    const { container } = render(grid(bigView(500, 31, 60).document));
    const rows = container.querySelector<HTMLElement>("section > div");
    expect(screen.getAllByRole("button")).toHaveLength(58);
    expect(screen.queryByText("Agent 31")).toBeNull();
    expect(screen.getByText("Agent 33")).toBeTruthy();
    expect(rows?.style.paddingTop).toBe(`${11 * 140}px`);
    // rows: ceil(500/3) = 167; first drawn row 11, 20 rows drawn -> 136 below.
    expect(rows?.style.paddingBottom).toBe(`${136 * 140}px`);
  });

  it("explains an empty monitor: no daemons (with Add daemon), no agents, and filters that hide everything (with Clear filters)", async () => {
    const onAddDaemon = vi.fn<() => void>();
    const onClearFilters = vi.fn<() => void>();
    const { rerender } = render(grid(emptyView().document, { onAddDaemon, onClearFilters }));
    expect(screen.getByText("No daemons yet. Add one from the Daemon menu.")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Add daemon" }));
    expect(onAddDaemon).toHaveBeenCalledTimes(1);

    const noAgents = sampleView().document;
    if (noAgents) {
      noAgents.cards = { total: 0, start: 0, virtualised: false, window: [] };
    }
    rerender(grid(noAgents, { onAddDaemon, onClearFilters }));
    expect(screen.getByText("No agents on the connected daemons.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Clear filters" })).toBeNull();

    const hidden = sampleView().document;
    if (hidden) {
      hidden.cards = { total: 0, start: 0, virtualised: false, window: [] };
      hidden.statusChips = [{ bucket: "error", count: 0, selected: true }];
    }
    rerender(grid(hidden, { onAddDaemon, onClearFilters }));
    expect(screen.getByText("No agents match the filters.")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(onClearFilters).toHaveBeenCalledTimes(1);
  });

  it("passes a card click up", async () => {
    const onSelect = vi.fn<(card: { agentId: string }) => void>();
    render(grid(richView(NOW).document, { onSelect }));
    await userEvent.click(screen.getByRole("button", { name: /Refactor parser/ }));
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ agentId: "r2", serverId: "srv-1" }));
  });
});
