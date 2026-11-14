import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { GroupHeader } from "../lib/types";
import { WorkspaceHeader } from "./WorkspaceHeader";

const group: GroupHeader = {
  key: "k",
  serverId: "srv-1",
  daemonName: "workstation",
  label: "a/app",
  path: "/work/a/app",
  count: 4,
  start: 0,
  needsAttention: false,
};

describe("WorkspaceHeader", () => {
  it("shows the label, the full path and the count; the daemon only when asked", () => {
    const { rerender } = render(<WorkspaceHeader group={group} showDaemon={false} />);
    expect(screen.getByText("a/app")).toBeTruthy();
    expect(screen.getByText("/work/a/app")).toBeTruthy();
    expect(screen.getByText("4")).toBeTruthy();
    expect(screen.queryByText("workstation")).toBeNull();
    expect(screen.queryByText("needs attention")).toBeNull();
    rerender(<WorkspaceHeader group={group} showDaemon />);
    expect(screen.getByText("workstation")).toBeTruthy();
  });

  it("warns when an agent in the group needs the engineer", () => {
    render(<WorkspaceHeader group={{ ...group, needsAttention: true }} showDaemon={false} />);
    expect(screen.getByText("needs attention")).toBeTruthy();
  });

  it("has no path line for the group without a workspace", () => {
    render(<WorkspaceHeader group={{ ...group, label: "No workspace", path: null }} showDaemon={false} />);
    expect(screen.getByText("No workspace")).toBeTruthy();
    expect(screen.queryByText("/work/a/app")).toBeNull();
  });

  it("renders hostile text as clipped plain text", () => {
    const html = "<img src=x onerror=alert(1)>";
    const { container } = render(<WorkspaceHeader group={{ ...group, label: `${html}${"A".repeat(1_000_000)}`, path: `/${html}${"B".repeat(1_000_000)}` }} showDaemon />);
    expect(container.querySelector("img")).toBeNull();
    expect((container.textContent ?? "").length).toBeLessThan(2_000);
  });
});
