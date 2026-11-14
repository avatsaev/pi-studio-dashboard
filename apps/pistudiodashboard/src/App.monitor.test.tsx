import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { App } from "./App";
import { emptyView, fakeBackend, sampleView } from "./test-support";
import { richView, withDrill } from "./test-support-monitor";

async function ready(view = sampleView()) {
  const fake = fakeBackend(view);
  render(<App backend={fake.backend} />);
  await screen.findByRole("heading", { name: "Pi Studio Dashboard" });
  return fake;
}

describe("App: card grid, filters and drill-down", () => {
  it("a card click is a registry call, agent.select", async () => {
    const fake = await ready(richView());
    await userEvent.click(screen.getByRole("button", { name: /Refactor parser/ }));
    expect(fake.calls).toEqual([["control", "engine.execute", { command: "agent.select", params: { serverId: "srv-1", agentId: "r2" } }]]);
  });

  it("a status chip click sends the new set of statuses, and clearing sends both empty lists", async () => {
    const view = richView();
    const chips = view.document?.statusChips ?? [];
    chips[1] = { bucket: "error", count: 1, selected: true };
    const fake = await ready(view);
    await userEvent.click(within(screen.getByRole("list", { name: "Status filters" })).getByRole("button", { name: /running/ }));
    expect(fake.calls).toEqual([["control", "engine.execute", { command: "view.filterStatuses", params: { statuses: ["error", "running"] } }]]);
    fake.calls.length = 0;
    await userEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(fake.calls).toEqual([
      ["control", "engine.execute", { command: "view.filterDaemons", params: { serverIds: [] } }],
      ["control", "engine.execute", { command: "view.filterStatuses", params: { statuses: [] } }],
    ]);
  });

  it("shows the totals in the status bar: 'N of M agents', tokens and cost", async () => {
    await ready(richView());
    expect(screen.getByRole("status").textContent).toBe("1 of 2 daemons connected · 2 of 8 agents · 13.1k tokens · cost 0.43");
  });

  it("the drill panel replaces the attention rail while open, and the rail returns when it closes", async () => {
    const fake = await ready(withDrill(richView()));
    expect(screen.getByLabelText("Agent details")).toBeTruthy();
    expect(screen.queryByLabelText("Attention rail")).toBeNull();
    act(() => fake.emit(richView()));
    expect(screen.queryByLabelText("Agent details")).toBeNull();
    expect(screen.getByLabelText("Attention rail")).toBeTruthy();
  });

  it("the close button runs agent.deselect", async () => {
    const fake = await ready(withDrill(richView()));
    await userEvent.click(screen.getByRole("button", { name: "Close agent details" }));
    expect(fake.calls).toEqual([["control", "engine.execute", { command: "agent.deselect", params: {} }]]);
  });

  it("Esc runs agent.deselect through the shortcut table while the drill is open", async () => {
    const fake = await ready(withDrill(richView()));
    await userEvent.keyboard("{Escape}");
    expect(fake.calls).toEqual([["invoke", "agent.deselect"]]);
  });

  it("Esc with no drill open does nothing, even when the command is listed but disabled", async () => {
    const fake = await ready(richView());
    await userEvent.keyboard("{Escape}");
    expect(fake.calls).toEqual([]);
    const disabled = withDrill(richView());
    for (const menu of disabled.menus) {
      for (const item of menu.items) {
        if (item.id === "agent.deselect") {
          item.enabled = false;
          item.reason = "no agent details are open";
        }
      }
    }
    act(() => fake.emit(disabled));
    await userEvent.keyboard("{Escape}");
    expect(fake.calls).toEqual([]);
  });

  it("Esc belongs to an open dialog: it does not also close the drill", async () => {
    const view = withDrill(richView());
    view.ui.dialog = { kind: "removeDaemon", entryId: "e1" };
    const fake = await ready(view);
    await userEvent.keyboard("{Escape}");
    // The dialog closes itself; the shortcut for the drill must not run besides.
    expect(fake.calls.filter((call) => call[0] === "invoke")).toEqual([]);
  });

  it("Esc that closes an open menu is handled by the menu: it does not also close the drill", async () => {
    const fake = await ready(withDrill(richView()));
    await userEvent.click(screen.getByRole("button", { name: "Agent" }));
    expect(screen.getByRole("menuitem", { name: /Close Agent Details/ })).toBeTruthy();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menuitem", { name: /Close Agent Details/ })).toBeNull();
    expect(fake.calls).toEqual([]);
    // With the menu closed, the next Esc closes the drill.
    await userEvent.keyboard("{Escape}");
    expect(fake.calls).toEqual([["invoke", "agent.deselect"]]);
  });

  it("an empty monitor offers 'Add daemon' as the daemon.add command", async () => {
    const fake = await ready(emptyView());
    await userEvent.click(screen.getByRole("button", { name: "Add daemon" }));
    expect(fake.calls).toEqual([["invoke", "daemon.add"]]);
  });
});
