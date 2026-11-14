import { describe, expect, it } from "vitest";

import { clip, compactNumber, formatClock, formatCost } from "./format";

describe("compactNumber", () => {
  it("keeps small numbers and abbreviates large ones", () => {
    expect(compactNumber(0)).toBe("0");
    expect(compactNumber(999)).toBe("999");
    expect(compactNumber(1000)).toBe("1k");
    expect(compactNumber(12_345)).toBe("12.3k");
    expect(compactNumber(123_456)).toBe("123k");
    expect(compactNumber(1_250_000)).toBe("1.3M");
    expect(compactNumber(2_000_000_000)).toBe("2B");
  });

  it("rolls a rounded-up thousand into the next unit and survives non-numbers", () => {
    expect(compactNumber(999_960)).toBe("1M");
    expect(compactNumber(Number.NaN)).toBe("—");
    expect(compactNumber(-12_345)).toBe("-12.3k");
  });
});

describe("formatCost", () => {
  it("is the number as supplied, without a currency", () => {
    expect(formatCost(0.42)).toBe("0.42");
    expect(formatCost(12)).toBe("12");
  });
});

describe("formatClock", () => {
  it("shows only the time today and the date besides on another day", () => {
    const now = new Date(2026, 10, 14, 15, 30).getTime();
    expect(formatClock(new Date(2026, 10, 14, 9, 5).getTime(), now)).toMatch(/9:05|09:05/);
    expect(formatClock(new Date(2026, 10, 12, 9, 5).getTime(), now)).toMatch(/Nov.*12/);
    expect(formatClock(Number.NaN, now)).toBe("");
  });
});

describe("clip", () => {
  it("cuts long text with an ellipsis and leaves short text alone", () => {
    expect(clip("short")).toBe("short");
    expect(clip("x".repeat(1_000_000)).length).toBe(401);
    expect(clip("abcdef", 3)).toBe("abc…");
  });
});
