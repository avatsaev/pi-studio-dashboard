import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DrillItem, DrillView } from "../lib/types";
import { card, drillItems } from "../test-support-monitor";
import { DrillDownPanel, FOLLOW_SLACK, isAtBottom } from "./DrillDownPanel";
import { resultSummary, TAIL_LINES } from "./DrillRows";

const NOW = Date.now();

function drillOf(more: Partial<DrillView> = {}): DrillView {
  return {
    key: { serverId: "srv-1", agentId: "a1" },
    header: card("a1", "Refactor parser", "running", { selected: true, usage: { tokens: 12_345, cost: 0.42, contextPercent: 38, takenAt: NOW - 1000 } }),
    items: drillItems(),
    truncatedAtStart: false,
    state: "live",
    stale: false,
    ...more,
  };
}

function panel(drill: DrillView, onClose: () => void = () => {}) {
  return <DrillDownPanel drill={drill} now={NOW} onClose={onClose} />;
}

describe("DrillDownPanel rows", () => {
  it("draws every row kind: assistant text with newlines, collapsed reasoning, tools, turn markers, error and system rows", () => {
    const { container } = render(panel(drillOf()));
    const body = within(screen.getByTestId("drill-body"));
    // `getByText` folds newlines, so the text is matched on the element: the line break is kept.
    expect(body.getByText((_, el) => el?.tagName === "P" && el.textContent === "I'll start with the tokenizer.\nThen the parser.")).toBeTruthy();
    const reasoning = container.querySelector("details");
    expect(reasoning?.open).toBe(false);
    expect(within(reasoning as HTMLElement).getByText("thinking")).toBeTruthy();
    for (const word of ["turn started", "turn failed", "turn completed"]) {
      expect(body.getByText(word)).toBeTruthy();
    }
    expect(body.getByText("mock stream error")).toBeTruthy();
    expect(body.getByText("Refactor the parser into two modules")).toBeTruthy();
    for (const label of ["src/lexer.rs", "cargo test", "src/parser.rs"]) {
      expect(body.getByText(label)).toBeTruthy();
    }
    expect(body.getByText("completed")).toBeTruthy();
    expect(body.getByText("running")).toBeTruthy();
    expect(body.getByText("error")).toBeTruthy();
  });

  it("shows a running tool's last 12 output lines and a finished one's result in a single line", () => {
    render(panel(drillOf()));
    const body = within(screen.getByTestId("drill-body"));
    const live = body.getByText(/test t29 \.\.\. ok/);
    expect(live.textContent?.split("\n")).toHaveLength(TAIL_LINES);
    expect(live.textContent).toContain("test t18 ... ok");
    expect(live.textContent).not.toContain("test t17 ... ok");
    // completed: the last non-empty line, not the whole output
    expect(body.getByText("pub struct Lexer {")).toBeTruthy();
    expect(body.queryByText("// lexer")).toBeNull();
    expect(body.getByText("error: file not found")).toBeTruthy();
    expect(resultSummary(["a", "b", "  ", ""])).toBe("b");
    expect(resultSummary(null)).toBe("");
  });

  it("renders hostile strings as plain text: no element is made from markup and nothing grows without bound", () => {
    const html = "<img src=x onerror=alert(1)><script>alert(2)</script>";
    const huge = "Z".repeat(5_000_000);
    const items: DrillItem[] = [
      { type: "assistant", text: html },
      { type: "assistant", text: huge },
      { type: "reasoning", text: html },
      { type: "tool", callId: "c", kind: "shell", label: html + huge, status: "running", outputTail: [html, huge] },
      { type: "error", message: html + huge },
      { type: "system", text: html + huge },
    ];
    const { container } = render(panel(drillOf({ items })));
    expect(container.querySelector("img[src='x'], script")).toBeNull();
    expect(container.textContent).toContain("<img src=x onerror=alert(1)>");
    expect(container.textContent?.length ?? 0).toBeLessThan(100_000);
  });

  it("holds at most the 2,000 rows it was given and says earlier output is not shown", () => {
    const items: DrillItem[] = Array.from({ length: 2000 }, (_, i) => ({ type: "assistant", text: `row ${i}` }));
    render(panel(drillOf({ items, truncatedAtStart: true })));
    const body = screen.getByTestId("drill-body");
    expect(body.querySelectorAll("p")).toHaveLength(2000 + 1);
    expect(within(body).getByText("earlier output not shown")).toBeTruthy();
    expect(within(body).getByText("row 1999")).toBeTruthy();
  });

  it("shows no 'earlier output' row for a short timeline", () => {
    render(panel(drillOf()));
    expect(screen.queryByText("earlier output not shown")).toBeNull();
  });
});

describe("DrillDownPanel states and header", () => {
  it("shows a loading state instead of rows", () => {
    render(panel(drillOf({ state: "loading", items: [] })));
    expect(screen.getByRole("status", { name: "Loading timeline" })).toBeTruthy();
    expect(screen.queryByText("turn started")).toBeNull();
  });

  it("shows a reloading indicator while refetching, keeping the rows", () => {
    render(panel(drillOf({ state: "refetching" })));
    expect(screen.getByText("Reloading…")).toBeTruthy();
    expect(screen.getByText("turn started")).toBeTruthy();
    expect(screen.queryByRole("status", { name: "Loading timeline" })).toBeNull();
  });

  it("shows the stale banner with the last known output when the daemon is disconnected", () => {
    render(panel(drillOf({ stale: true })));
    expect(screen.getByText("daemon disconnected — showing last known output")).toBeTruthy();
    expect(screen.getByText("turn started")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toContain("daemon disconnected");
  });

  it("says so for a live timeline with no rows yet", () => {
    render(panel(drillOf({ items: [] })));
    expect(screen.getByText("No output yet.")).toBeTruthy();
  });

  it("repeats the card in the header and closes through the close button", async () => {
    const onClose = vi.fn<() => void>();
    render(panel(drillOf(), onClose));
    const header = within(screen.getByRole("banner"));
    expect(header.getByRole("heading", { name: "Refactor parser" })).toBeTruthy();
    expect(header.getByText(/workstation · mock/)).toBeTruthy();
    expect(header.getByText(/12\.3k tokens · cost 0\.42 · 38% context/)).toBeTruthy();
    expect(header.getByRole("img", { name: "running" })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Close agent details" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("falls back to the agent id when the agent is not in the monitor, and ships no dead controls", () => {
    render(panel(drillOf({ header: null })));
    expect(screen.getByRole("heading", { name: "a1" })).toBeTruthy();
    // Only the close button: the Interrupt / Open-in-web-UI / dialog slots stay empty until sprint-007.
    expect(screen.getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["Close agent details"]);
    for (const slot of document.querySelectorAll("[data-slot]")) {
      expect(slot.childElementCount).toBe(0);
    }
  });
});

describe("auto-follow", () => {
  // jsdom has no layout: one shared scroll box.
  const box = { top: 0, height: 1000, view: 200 };
  beforeEach(() => {
    box.top = 0;
    box.height = 1000;
    vi.spyOn(Element.prototype, "scrollHeight", "get").mockImplementation(() => box.height);
    vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(() => box.view);
    vi.spyOn(Element.prototype, "scrollTop", "get").mockImplementation(() => box.top);
    vi.spyOn(Element.prototype, "scrollTop", "set").mockImplementation((value: number) => {
      box.top = value;
    });
  });
  afterEach(() => vi.restoreAllMocks());

  it("knows when it is at the bottom, within a small slack", () => {
    expect(isAtBottom({ scrollHeight: 1000, scrollTop: 800, clientHeight: 200 })).toBe(true);
    expect(isAtBottom({ scrollHeight: 1000, scrollTop: 800 - FOLLOW_SLACK, clientHeight: 200 })).toBe(true);
    expect(isAtBottom({ scrollHeight: 1000, scrollTop: 800 - FOLLOW_SLACK - 1, clientHeight: 200 })).toBe(false);
  });

  it("follows new rows at the bottom, does not yank a user who scrolled up, and follows again after 'Jump to latest'", async () => {
    const more = (n: number): DrillItem[] => [...drillItems(), ...Array.from({ length: n }, (_, i): DrillItem => ({ type: "assistant", text: `more ${i}` }))];
    const { rerender } = render(panel(drillOf()));
    // opens at the newest row
    expect(box.top).toBe(1000);
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();

    box.height = 1200;
    rerender(panel(drillOf({ items: more(2) })));
    expect(box.top).toBe(1200);

    // the user scrolls up: new rows leave the scroll position alone
    box.top = 300;
    fireEvent.scroll(screen.getByTestId("drill-body"));
    expect(screen.getByRole("button", { name: "Jump to latest" })).toBeTruthy();
    box.height = 1500;
    rerender(panel(drillOf({ items: more(5) })));
    expect(box.top).toBe(300);

    await userEvent.click(screen.getByRole("button", { name: "Jump to latest" }));
    expect(box.top).toBe(1500);
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();
    box.height = 1800;
    rerender(panel(drillOf({ items: more(8) })));
    expect(box.top).toBe(1800);

    // scrolling back to the bottom by hand also resumes following
    box.top = 100;
    fireEvent.scroll(screen.getByTestId("drill-body"));
    box.top = 1800 - 200;
    fireEvent.scroll(screen.getByTestId("drill-body"));
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();
  });

  it("starts at the newest row again for another agent", () => {
    const { rerender } = render(panel(drillOf()));
    box.top = 100;
    fireEvent.scroll(screen.getByTestId("drill-body"));
    expect(screen.getByRole("button", { name: "Jump to latest" })).toBeTruthy();
    rerender(panel(drillOf({ key: { serverId: "srv-1", agentId: "a2" } })));
    expect(box.top).toBe(1000);
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();
  });
});
