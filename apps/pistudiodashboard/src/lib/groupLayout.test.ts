import { describe, expect, it } from "vitest";

import { groupOffsets, planGroups, visibleGroupedRange, type GroupMetrics } from "./groupLayout";

const m = (cols: number): GroupMetrics => ({ cols, rowHeight: 140, headerHeight: 32, gap: 12 });
const sum = (xs: readonly number[]) => xs.reduce((a, b) => a + b, 0);

describe("groupOffsets", () => {
  it("a single group is its title plus its rows", () => {
    expect(groupOffsets([7], m(3))).toEqual({ top: [0], height: [32 + 3 * 140], total: 32 + 3 * 140 });
  });

  it("stacks sections with one gap between them", () => {
    const o = groupOffsets([1, 4, 3], m(3));
    expect(o.height).toEqual([172, 32 + 2 * 140, 172]);
    expect(o.top).toEqual([0, 172 + 12, 172 + 12 + 312 + 12]);
    expect(o.total).toBe(172 + 12 + 312 + 12 + 172);
    expect(groupOffsets([], m(3)).total).toBe(0);
  });
});

describe("visibleGroupedRange", () => {
  it("equals the flat maths for one group: whole rows from the first visible, plus one", () => {
    const layout = [300];
    const o = groupOffsets(layout, m(3));
    // first row starts below the title: top 0 + 32 + 12 = 44
    expect(visibleGroupedRange(0, 600, layout, o, m(3))).toEqual({ start: 0, count: 15 });
    expect(visibleGroupedRange(44 + 140 * 10, 600, layout, o, m(3))).toEqual({ start: 30, count: 18 });
  });

  it("many one-card groups: each title takes room, so fewer cards fit", () => {
    const layout = Array.from({ length: 50 }, () => 1);
    const o = groupOffsets(layout, m(3));
    const r = visibleGroupedRange(0, 600, layout, o, m(3));
    expect(r.start).toBe(0);
    expect(r.count).toBeLessThan(18);
    expect(r.count).toBeGreaterThan(3);
  });

  it("a section taller than the viewport reports only the visible rows of it", () => {
    const layout = [90, 5];
    const o = groupOffsets(layout, m(3));
    const r = visibleGroupedRange(1000, 300, layout, o, m(3));
    expect(r.start % 3).toBe(0);
    expect(r.start).toBeGreaterThan(0);
    expect(r.start + r.count).toBeLessThan(90);
  });

  it("at the bottom it ends at the last card and never beyond", () => {
    const layout = [4, 4, 4];
    for (const cols of [1, 3, 5]) {
      const o = groupOffsets(layout, m(cols));
      const r = visibleGroupedRange(o.total, 600, layout, o, m(cols));
      expect(r.start + r.count).toBeLessThanOrEqual(12);
      const top = visibleGroupedRange(0, 600, layout, o, m(cols));
      expect(top.start).toBe(0);
    }
  });

  it("counts the first row of a section whose title is visible but whose cards are not yet", () => {
    const layout = [3, 3];
    const o = groupOffsets(layout, m(3));
    // Second section's title is at its top; its cards start 44px lower. View ends 10px into the title.
    const second = o.top[1] ?? 0;
    const r = visibleGroupedRange(second - 100, 110, layout, o, m(3));
    expect(r.start + r.count).toBeGreaterThan(3);
  });

  it("scrolling in small steps reports windows that cover every card", () => {
    const layout = Array.from({ length: 40 }, (_, i) => (i % 7) + 1);
    const total = sum(layout);
    for (const cols of [1, 3, 5]) {
      const metrics = m(cols);
      const o = groupOffsets(layout, metrics);
      const seen = new Set<number>();
      for (let y = 0; y <= o.total; y += 37) {
        const r = visibleGroupedRange(y, 500, layout, o, metrics);
        expect(r.count).toBeGreaterThan(0);
        for (let i = r.start; i < Math.min(total, r.start + r.count); i += 1) {
          seen.add(i);
        }
      }
      expect(seen.size).toBe(total);
    }
  });
});

describe("planGroups", () => {
  /** A group's drawn height: title, gap, and its card grid (spacers plus drawn rows). */
  function drawnHeight(count: number, p: { cardFrom: number; cardTo: number; padTop: number; padBottom: number }, metrics: GroupMetrics) {
    const rows = Math.ceil((p.cardTo - p.cardFrom) / metrics.cols);
    const grid = p.padTop + p.padBottom + (rows > 0 ? rows * metrics.rowHeight - metrics.gap : 0);
    expect(count).toBeGreaterThan(0);
    return metrics.headerHeight + metrics.gap + grid;
  }

  it("never leaves a gap or an overlap: drawn sections and spacers add up to the whole list", () => {
    const layout = Array.from({ length: 60 }, (_, i) => (i % 9) + 1);
    const total = sum(layout);
    for (const cols of [1, 2, 3, 5]) {
      const metrics = m(cols);
      const o = groupOffsets(layout, metrics);
      for (const [start, end] of [
        [0, 40],
        [13, 77],
        [100, 220],
        [total - 30, total],
        [0, total],
      ] as const) {
        const firstGroup = layout.findIndex((_, i) => sum(layout.slice(0, i + 1)) > start);
        const headers: { index: number; start: number }[] = [];
        let at = 0;
        layout.forEach((count, i) => {
          if (i >= firstGroup && at < end && at + count > start) {
            headers.push({ index: i, start: at });
          }
          at += count;
        });
        const plan = planGroups(headers, start, end, layout, o, metrics);
        expect(plan).not.toBeNull();
        if (!plan) {
          continue;
        }
        const heights = plan.groups.map((g) => drawnHeight(layout[g.index] ?? 0, g, metrics));
        const drawn = sum(heights) + metrics.gap * Math.max(0, plan.groups.length - 1);
        expect(plan.padTop + drawn + plan.padBottom).toBe(o.total);
        for (const g of plan.groups) {
          const present = headers.find((h) => h.start === sum(layout.slice(0, g.index)));
          expect(present).toBeDefined();
          // Only cards that were sent are drawn.
          const base = sum(layout.slice(0, g.index));
          expect(base + g.cardFrom).toBeGreaterThanOrEqual(Math.min(start, base + (layout[g.index] ?? 0)));
          expect(base + g.cardTo).toBeLessThanOrEqual(Math.max(end, base + g.cardFrom));
        }
      }
    }
  });

  it("draws a partial row only at the end of its group", () => {
    const layout = [7];
    const metrics = m(3);
    const o = groupOffsets(layout, metrics);
    const whole = planGroups([{ index: 0, start: 0 }], 0, 7, layout, o, metrics);
    expect(whole?.groups[0]).toMatchObject({ cardFrom: 0, cardTo: 7, padTop: 0, padBottom: 0 });
    const cut = planGroups([{ index: 0, start: 0 }], 0, 5, layout, o, metrics);
    expect(cut?.groups[0]).toMatchObject({ cardFrom: 0, cardTo: 3, padBottom: 2 * 140 });
  });

  it("a group whose cards are all above or below the slice becomes one exact spacer", () => {
    const layout = [3, 3];
    const metrics = m(3);
    const o = groupOffsets(layout, metrics);
    // The slice starts inside the first group's only row: no whole row of it is present.
    const plan = planGroups([{ index: 0, start: 0 }, { index: 1, start: 3 }], 2, 6, layout, o, metrics);
    expect(plan?.groups[0]).toMatchObject({ cardFrom: 3, cardTo: 3, padTop: 140 - 12, padBottom: 0 });
    expect(plan?.groups[1]).toMatchObject({ cardFrom: 0, cardTo: 3, padTop: 0, padBottom: 0 });
  });

  it("rejects headers that do not match the layout", () => {
    const headers = [{ index: 0, start: 0 }, { index: 2, start: 3 }];
    expect(planGroups(headers, 0, 6, [3, 3, 3], groupOffsets([3, 3, 3], m(3)), m(3))).toBeNull();
  });

  it("an empty header list is one big spacer", () => {
    expect(planGroups([], 0, 0, [3], groupOffsets([3], m(3)), m(3))).toEqual({ padTop: 0, padBottom: 32 + 140, groups: [] });
  });
});

describe("collapsed groups (0 cards in the layout)", () => {
  it("are a title and nothing else", () => {
    const o = groupOffsets([3, 0, 0, 3], m(3));
    expect(o.height).toEqual([172, 32, 32, 172]);
    expect(o.top).toEqual([0, 172 + 12, 172 + 12 + 32 + 12, 172 + 12 + 32 + 12 + 32 + 12]);
    expect(groupOffsets([0, 0], m(3)).total).toBe(32 + 12 + 32);
  });

  it("ask for the card index where they sit when only their titles are on screen", () => {
    const layout = [3, ...Array.from({ length: 100 }, () => 0), 3];
    const o = groupOffsets(layout, m(3));
    const middle = (o.top[50] ?? 0) - 10;
    expect(visibleGroupedRange(middle, 300, layout, o, m(3))).toEqual({ start: 3, count: 3 });
    // Past everything: the end of the list.
    expect(visibleGroupedRange(o.total + 500, 300, layout, o, m(3)).start).toBeLessThanOrEqual(6);
  });

  it("still cover every card when scrolling through a mix", () => {
    const layout = Array.from({ length: 60 }, (_, i) => (i % 3 === 0 ? 0 : (i % 5) + 1));
    const total = sum(layout);
    const metrics = m(3);
    const o = groupOffsets(layout, metrics);
    const seen = new Set<number>();
    for (let y = 0; y <= o.total; y += 29) {
      const r = visibleGroupedRange(y, 500, layout, o, metrics);
      for (let i = r.start; i < Math.min(total, r.start + r.count); i += 1) {
        seen.add(i);
      }
    }
    expect(seen.size).toBe(total);
  });

  it("plan as header-only groups with no spacers, and the list height still adds up", () => {
    const layout = [3, 0, 3, 0, 0, 3];
    const metrics = m(3);
    const o = groupOffsets(layout, metrics);
    // The slice is [0, 9): every group is a header.
    const headers = [
      { index: 0, start: 0 },
      { index: 1, start: 3 },
      { index: 2, start: 3 },
      { index: 3, start: 6 },
      { index: 4, start: 6 },
      { index: 5, start: 6 },
    ];
    const plan = planGroups(headers, 0, 9, layout, o, metrics);
    expect(plan?.groups.filter((g) => g.cardTo === g.cardFrom).map((g) => g.index)).toEqual([1, 3, 4]);
    expect(plan?.groups.filter((g) => g.cardTo === g.cardFrom).every((g) => g.padTop === 0 && g.padBottom === 0)).toBe(true);
    const drawn = (plan?.groups ?? []).reduce((acc, g) => acc + 32 + (layout[g.index] ? 12 + 140 - 12 : 0), 0) + 12 * 5;
    expect((plan?.padTop ?? 0) + drawn + (plan?.padBottom ?? 0)).toBe(o.total);
  });
});
