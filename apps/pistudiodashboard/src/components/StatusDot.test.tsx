import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { AgentStatus } from "../lib/types";
import { reducedMotionCss } from "../test-support";
import { dotStyle, StatusDot, type Attention, type DotStyle } from "./StatusDot";

type Row = [string, Attention, Pick<DotStyle, "shape" | "tone" | "pulse">, string];

describe("dotStyle (the single status -> dot translation)", () => {
  it.each<Row>([
    ["running", null, { shape: "ring", tone: "accentBright", pulse: false }, "running"],
    ["initializing", null, { shape: "flat", tone: "accent", pulse: false }, "initializing"],
    ["idle", null, { shape: "flat", tone: "muted", pulse: false }, "idle"],
    ["error", null, { shape: "flat", tone: "danger", pulse: false }, "error"],
    ["closed", null, { shape: "flat", tone: "muted", pulse: false }, "closed"],
    ["unknown", null, { shape: "outline", tone: "muted", pulse: false }, "unknown"],
    ["from-a-newer-daemon", null, { shape: "outline", tone: "muted", pulse: false }, "unknown"],
    ["", null, { shape: "outline", tone: "muted", pulse: false }, "unknown"],
  ])("%s with no attention", (status, attention, expected, word) => {
    const dot = dotStyle(status, attention);
    expect(dot).toMatchObject(expected);
    expect(dot.label).toBe(word);
  });

  it.each(["running", "initializing", "idle", "error", "closed", "unknown", "weird"])("a pending dialog overrides %s: warning, flat, pulsing", (status) => {
    expect(dotStyle(status, "dialog")).toMatchObject({ shape: "flat", tone: "warning", pulse: true });
  });

  it.each(["running", "initializing", "idle", "error", "closed", "unknown", "weird"])("an errored agent overrides %s: danger, flat, still", (status) => {
    expect(dotStyle(status, "error")).toMatchObject({ shape: "flat", tone: "danger", pulse: false });
  });

  it("is never colour-only: the label always carries the status word", () => {
    expect(dotStyle("running", "dialog").label).toBe("running, needs a response");
    expect(dotStyle("idle", "error").label).toBe("idle, errored");
    expect(dotStyle("error", "error").label).toBe("error");
    expect(dotStyle("weird", "dialog").label).toBe("unknown, needs a response");
  });
});

describe("StatusDot", () => {
  it.each<[AgentStatus, string, string]>([
    ["running", "ring", "accentBright"],
    ["initializing", "flat", "accent"],
    ["idle", "flat", "muted"],
    ["error", "flat", "danger"],
    ["closed", "flat", "muted"],
    ["unknown", "outline", "muted"],
  ])("draws %s as a %s %s dot with its word as the accessible name", (status, shape, tone) => {
    render(<StatusDot status={status} />);
    const dot = screen.getByRole("img", { name: status });
    expect(dot.dataset.shape).toBe(shape);
    expect(dot.dataset.tone).toBe(tone);
  });

  it("draws an unknown status string as an outlined muted dot labelled unknown, and does not throw", () => {
    render(<StatusDot status={"hibernating" as AgentStatus} />);
    const dot = screen.getByRole("img", { name: "unknown" });
    expect(dot.dataset.shape).toBe("outline");
    expect(dot.dataset.tone).toBe("muted");
  });

  it("lets a pending dialog win over a running ring, and an error over idle", () => {
    const { rerender } = render(<StatusDot status="running" attention="dialog" />);
    expect(screen.getByRole("img").dataset).toMatchObject({ shape: "flat", tone: "warning" });
    rerender(<StatusDot status="idle" attention="error" />);
    expect(screen.getByRole("img").dataset).toMatchObject({ shape: "flat", tone: "danger" });
  });

  it("takes an explicit label", () => {
    render(<StatusDot status="idle" label="daemon asleep" />);
    expect(screen.getByRole("img", { name: "daemon asleep" })).toBeTruthy();
  });
});

describe("StatusDot under prefers-reduced-motion", () => {
  const still = reducedMotionCss("./components/StatusDot.module.css");

  it("holds the running ring and the pulse still", () => {
    expect(still).toMatch(/\.ring[\s\S]*\.pulse[\s\S]*animation:\s*none/);
  });
});
