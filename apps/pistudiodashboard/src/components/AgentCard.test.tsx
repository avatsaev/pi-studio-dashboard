import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { card } from "../test-support-monitor";
import { AgentCard, usageParts } from "./AgentCard";

const NOW = new Date(2026, 10, 14, 15, 30).getTime();

describe("AgentCard", () => {
  it("draws the title, daemon, model, current tool, last output line and usage as plain text", () => {
    const running = card("a", "Refactor parser", "running", {
      currentTool: { label: "cargo test", status: "running" },
      lastOutputLine: "running 12 tests",
      usage: { tokens: 12_345, cost: 0.42, contextPercent: 38.4, takenAt: NOW - 1000 },
    });
    render(<AgentCard card={running} now={NOW} onSelect={() => {}} />);
    const button = screen.getByRole("button", { name: /Refactor parser/ });
    for (const text of ["Refactor parser", "workstation", "mock", "cargo test", "running 12 tests", "12.3k tokens", "cost 0.42", "38% context"]) {
      expect(within(button).getByText(text)).toBeTruthy();
    }
    expect(button.querySelector('[class*="status"]')?.textContent).toBe("running");
    expect(button.textContent).not.toContain("as of");
  });

  it("shows the workspace after the daemon, with the full path as its tooltip, unless the header already says it", () => {
    const withCwd = card("a", "t", "idle", { cwd: "/work/a/app", workspaceLabel: "a/app" });
    const { rerender } = render(<AgentCard card={withCwd} now={NOW} onSelect={() => {}} />);
    const workspace = screen.getByText("a/app");
    expect(workspace.getAttribute("title")).toBe("/work/a/app");
    expect(workspace.previousElementSibling?.textContent).toBe("workstation");
    rerender(<AgentCard card={withCwd} now={NOW} onSelect={() => {}} showWorkspace={false} />);
    expect(screen.queryByText("a/app")).toBeNull();
    rerender(<AgentCard card={card("a", "t", "idle", { cwd: null, workspaceLabel: "No workspace" })} now={NOW} onSelect={() => {}} />);
    expect(screen.getByText("No workspace")).toBeTruthy();
  });

  it("reads '—' for tokens and cost without usage, and 'as of <time>' when the snapshot is old", () => {
    const { rerender } = render(<AgentCard card={card("a", "t", "idle")} now={NOW} onSelect={() => {}} />);
    expect(screen.getByText("— tokens")).toBeTruthy();
    expect(screen.getByText("cost —")).toBeTruthy();
    rerender(<AgentCard card={card("a", "t", "idle", { usage: { tokens: 800, cost: 1.5, contextPercent: null, takenAt: NOW - 3_600_000 } })} now={NOW} onSelect={() => {}} />);
    expect(screen.getByText(/as of .*2:30|as of .*14:30/)).toBeTruthy();
    expect(screen.queryByText(/context/)).toBeNull();
  });

  it("splits usage into parts: compact tokens, bare cost, rounded context, fresh = no 'as of'", () => {
    expect(usageParts("none", NOW)).toEqual({ tokens: "—", cost: "—", context: null, asOf: null });
    expect(usageParts({ tokens: null, cost: null, contextPercent: null, takenAt: NOW }, NOW)).toEqual({ tokens: "—", cost: "—", context: null, asOf: null });
    expect(usageParts({ tokens: 1_500_000, cost: 12, contextPercent: 99.6, takenAt: NOW - 29_000 }, NOW)).toEqual({ tokens: "1.5M", cost: "12", context: "100%", asOf: null });
  });

  it("shows the status word, not only a colour, and each status's dot", () => {
    const dots: [Parameters<typeof card>[2], string, string][] = [
      ["initializing", "flat", "accent"],
      ["idle", "flat", "muted"],
      ["running", "ring", "accentBright"],
      ["error", "flat", "danger"],
      ["closed", "flat", "muted"],
    ];
    for (const [status, shape, tone] of dots) {
      const { unmount } = render(<AgentCard card={card("a", "t", status)} now={NOW} onSelect={() => {}} />);
      const dot = screen.getByRole("img", { name: status });
      expect([status, dot.dataset.shape, dot.dataset.tone]).toEqual([status, shape, tone]);
      expect(screen.getByRole("button").querySelector('[class*="status"]')?.textContent).toBe(status);
      unmount();
    }
  });

  it("overrides the dot with warning for a pending dialog and shows the pending count", () => {
    render(<AgentCard card={card("a", "Approve?", "idle", { attention: { pendingDialogs: 2, errored: false } })} now={NOW} onSelect={() => {}} />);
    expect(screen.getByRole("img", { name: "idle, needs a response" }).dataset.tone).toBe("warning");
    expect(screen.getByText("2 pending")).toBeTruthy();
  });

  it("dims a closed or stale card and says when the daemon went stale", () => {
    const { rerender } = render(<AgentCard card={card("a", "t", "closed")} now={NOW} onSelect={() => {}} />);
    expect(screen.getByRole("button").className).toContain("dimmed");
    expect(screen.queryByText(/stale/)).toBeNull();
    rerender(<AgentCard card={card("a", "t", "running", { dimmed: true, stale: true, staleSince: NOW - 600_000 })} now={NOW} onSelect={() => {}} />);
    expect(screen.getByRole("button").className).toContain("dimmed");
    expect(screen.getByText(/stale since .*(3:20|15:20)/)).toBeTruthy();
  });

  it("renders hostile strings as clipped plain text: no element is created from markup", () => {
    const html = "<img src=x onerror=alert(1)><script>alert(2)</script>";
    const hostile = card("a", `${html}${"A".repeat(1_000_000)}`, "running", {
      daemonName: html,
      lastOutputLine: html.repeat(1000),
      currentTool: { label: html.repeat(1000), status: "running" },
    });
    const { container } = render(<AgentCard card={hostile} now={NOW} onSelect={() => {}} />);
    expect(container.querySelector("img[src='x'], script")).toBeNull();
    expect(container.textContent?.length ?? 0).toBeLessThan(2000);
    expect(screen.getByRole("button").textContent).toContain("<img src=x onerror=alert(1)>");
    const titleLengths = [...container.querySelectorAll("[title]")].map((el) => el.getAttribute("title")?.length ?? 0);
    expect(Math.max(...titleLengths)).toBeLessThan(500);
  });

  it("selects on click and shows the selected state it was given", async () => {
    const onSelect = vi.fn<(c: ReturnType<typeof card>) => void>();
    const c = card("a", "Pick me", "idle");
    const { rerender } = render(<AgentCard card={c} now={NOW} onSelect={onSelect} />);
    expect(screen.getByRole("button").getAttribute("aria-pressed")).toBe("false");
    await userEvent.click(screen.getByRole("button"));
    expect(onSelect).toHaveBeenCalledWith(c);
    rerender(<AgentCard card={{ ...c, selected: true }} now={NOW} onSelect={onSelect} />);
    expect(screen.getByRole("button").getAttribute("aria-pressed")).toBe("true");
  });
});
