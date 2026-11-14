import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import type { Diagnostics } from "../lib/types";
import { fakeBackend } from "../test-support";
import { DiagnosticsPanel } from "./DiagnosticsPanel";

function data(more: Partial<Diagnostics> = {}): Diagnostics {
  return {
    entryId: "e1",
    name: "workstation",
    host: "10.0.0.5",
    port: 6790,
    tls: false,
    conn: "live",
    since: 1_700_000_000_000,
    lastError: { kind: "unreachable", message: "connection refused", at: 1_699_999_000_000 },
    nextRetryInMs: null,
    reconnectCount: 3,
    connectedSince: 1_700_000_000_000,
    messagesPerSecond: 12,
    messagesTotal: 4567,
    unknownTypes: [
      { name: "future_a", count: 9 },
      { name: "future_b", count: 2 },
    ],
    unknownTypeCount: 11,
    unknownEnumValues: 1,
    malformed: 2,
    binaryFramesIgnored: 0,
    framesTooLarge: 1,
    droppedEvents: 0,
    rtt: { lastMs: 7, averageMs: 9 },
    server: { serverId: "srv-1", hostname: "devbox", version: "0.0.115", featuresPresent: ["daemonObservability"], featuresMissing: [] },
    duplicateOf: null,
    degraded: [],
    health: { reachable: true, at: 1_700_000_001_000 },
    ...more,
  };
}

describe("DiagnosticsPanel", () => {
  it("shows last error, reconnects, rate, unknown and malformed counts, RTT and the real version", () => {
    render(<DiagnosticsPanel backend={fakeBackend(null).backend} data={data()} />);
    expect(screen.getByRole("heading", { name: "Diagnostics · workstation" })).toBeTruthy();
    const connection = within(screen.getByRole("region", { name: "Connection" }));
    expect(connection.getByText(/unreachable: connection refused/)).toBeTruthy();
    expect(connection.getByText("3")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Server" })).getByText("0.0.115")).toBeTruthy();
    const counters = within(screen.getByRole("region", { name: "Counters" }));
    expect(counters.getByText("12")).toBeTruthy();
    expect(counters.getByText("7 ms / 9 ms")).toBeTruthy();
    expect(counters.getByText("2")).toBeTruthy();
  });

  it("lists the unknown types as given, most seen first", () => {
    render(<DiagnosticsPanel backend={fakeBackend(null).backend} data={data()} />);
    const items = within(screen.getByRole("region", { name: "Unknown message types" })).getAllByRole("listitem");
    expect(items.map((i) => i.textContent)).toEqual(["future_a9", "future_b2"]);
  });

  it("explains no_observability inside the panel and draws missing optional fields as —", () => {
    const sparse = data({
      degraded: ["no_observability"],
      lastError: null,
      health: null,
      connectedSince: null,
      unknownTypes: [],
      rtt: { lastMs: null, averageMs: null },
      server: { serverId: null, hostname: null, version: null, featuresPresent: [], featuresMissing: ["daemonObservability"] },
    });
    render(<DiagnosticsPanel backend={fakeBackend(null).backend} data={sparse} />);
    expect(screen.getByRole("status").textContent).toBe("Health and logs need a newer pi-studio daemon.");
    expect(within(screen.getByRole("region", { name: "Counters" })).getByText("— / —")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Server" })).getAllByText("—").length).toBeGreaterThanOrEqual(3);
    expect(screen.getByText("none", { selector: "p" })).toBeTruthy();
  });

  it("Esc and the Close button clear the diagnostics target", async () => {
    const fake = fakeBackend(null);
    render(<DiagnosticsPanel backend={fake.backend} data={data()} />);
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    await userEvent.keyboard("{Escape}");
    expect(fake.calls).toEqual([
      ["control", "ui.set", { diagnosticsTarget: null }],
      ["control", "ui.set", { diagnosticsTarget: null }],
    ]);
  });
});
