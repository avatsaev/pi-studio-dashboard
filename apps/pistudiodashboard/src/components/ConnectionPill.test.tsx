import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ConnState } from "../lib/types";
import { reducedMotionCss } from "../test-support";
import { ConnectionPill, pillSpec, type PillSpec } from "./ConnectionPill";

describe("pillSpec", () => {
  it.each<[ConnState, PillSpec]>([
    ["probing", { tone: "accent", label: "Connecting…", spinner: true }],
    ["connecting", { tone: "accent", label: "Connecting…", spinner: true }],
    ["handshaking", { tone: "accent", label: "Connecting…", spinner: true }],
    ["syncing", { tone: "accent", label: "Connecting…", spinner: true }],
    ["live", { tone: "success", label: "Connected", spinner: false }],
    ["auth_failed", { tone: "danger", label: "Auth failed", spinner: false }],
    ["host_rejected", { tone: "danger", label: "Host not allowed", spinner: false }],
    ["idle", { tone: "muted", label: "Disconnected", spinner: false }],
    ["disabled", { tone: "muted", label: "Disabled", spinner: false }],
  ])("%s", (conn, expected) => {
    expect(pillSpec(conn)).toEqual(expected);
  });

  it("backoff: danger Unreachable when never live; warning Reconnecting with the next try once it was", () => {
    expect(pillSpec("backoff")).toEqual({ tone: "danger", label: "Unreachable", spinner: false });
    expect(pillSpec("backoff", { wasLive: false, retryInSeconds: 4 })).toEqual({ tone: "danger", label: "Unreachable", spinner: false });
    expect(pillSpec("backoff", { wasLive: true, retryInSeconds: 4 })).toEqual({ tone: "warning", label: "Reconnecting · next try in 4s", spinner: false });
    expect(pillSpec("backoff", { wasLive: true })).toEqual({ tone: "warning", label: "Reconnecting…", spinner: false });
  });

  it("renders an unknown state muted and never throws", () => {
    expect(pillSpec("quantum")).toEqual({ tone: "muted", label: "Unknown", spinner: false });
    expect(pillSpec("")).toEqual({ tone: "muted", label: "Unknown", spinner: false });
  });
});

describe("ConnectionPill", () => {
  it("shows the label and a spinner only while connecting", () => {
    const { container, rerender } = render(<ConnectionPill conn="connecting" />);
    expect(screen.getByText("Connecting…")).toBeTruthy();
    expect(container.querySelector('[class*="spinner"]')).not.toBeNull();
    rerender(<ConnectionPill conn="live" />);
    expect(screen.getByText("Connected")).toBeTruthy();
    expect(container.querySelector('[class*="spinner"]')).toBeNull();
    expect(container.querySelector('[class*="mark"]')).not.toBeNull();
  });

  it("shows the unencrypted warning badge only when asked", () => {
    const { rerender } = render(<ConnectionPill conn="live" />);
    expect(screen.queryByText("unencrypted")).toBeNull();
    rerender(<ConnectionPill conn="live" unencrypted />);
    expect(screen.getByText("unencrypted")).toBeTruthy();
  });

  it("draws an unknown state as a muted pill", () => {
    render(<ConnectionPill conn="quantum" />);
    expect(screen.getByText("Unknown")).toBeTruthy();
  });

  it("stops the spinner under prefers-reduced-motion", () => {
    expect(reducedMotionCss("./components/ConnectionPill.module.css")).toMatch(/\.spinner[\s\S]*animation:\s*none/);
  });
});
