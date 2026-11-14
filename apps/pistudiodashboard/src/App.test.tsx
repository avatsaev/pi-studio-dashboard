import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { emptyView, fakeBackend, sampleView } from "./test-support";

describe("App", () => {
  it("draws the empty monitor", async () => {
    render(<App backend={fakeBackend(emptyView()).backend} />);
    expect(await screen.findByRole("heading", { name: "Pi Studio Dashboard" })).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Agents" })).getByText("No daemons yet. Add one from the Daemon menu.")).toBeTruthy();
    expect(screen.getByLabelText("Attention rail")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toContain("0 of 0 agents");
  });

  it("draws the daemon chips and the totals label as sent", async () => {
    render(<App backend={fakeBackend(sampleView()).backend} />);
    const chips = within(await screen.findByRole("list", { name: "Daemons" }));
    expect(chips.getByText("workstation")).toBeTruthy();
    expect(chips.getByText("Connected")).toBeTruthy();
    expect(chips.getByText("Reconnecting…")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("1 of 2 daemons connected · 0 of 4 agents");
    expect(screen.getByText("Refactor parser")).toBeTruthy();
  });

  it("draws each card's status dot from its status and attention", async () => {
    render(<App backend={fakeBackend(sampleView()).backend} />);
    const agents = within(await screen.findByRole("region", { name: "Agents" }));
    // a1 running; a2 idle with a pending dialog -> warning; a3 error (errored); a4 closed.
    expect(agents.getByRole("img", { name: "running" }).dataset.shape).toBe("ring");
    expect(agents.getByRole("img", { name: "idle, needs a response" }).dataset.tone).toBe("warning");
    expect(agents.getByRole("img", { name: "error" }).dataset.tone).toBe("danger");
    expect(agents.getByRole("img", { name: "closed" }).dataset.tone).toBe("muted");
  });

  it("draws view.ui.toasts and calls nothing on the backend", async () => {
    const view = sampleView();
    view.ui.toasts = [{ id: 1, kind: "info", text: "Answered elsewhere: Allow" }];
    const fake = fakeBackend(view);
    render(<App backend={fake.backend} />);
    expect(await screen.findByText("Answered elsewhere: Allow")).toBeTruthy();
    expect(fake.calls).toEqual([]);
    const next = sampleView();
    next.ui.toasts = [...view.ui.toasts, { id: 2, kind: "warning", text: "second" }];
    act(() => fake.emit(next));
    expect(screen.getByText("second")).toBeTruthy();
    expect(fake.calls).toEqual([]);
  });

  it("says there are no agents when daemons exist but cards do not", async () => {
    const view = sampleView();
    if (view.document) {
      view.document.cards = { total: 0, start: 0, virtualised: false, window: [] };
    }
    render(<App backend={fakeBackend(view).backend} />);
    expect(await screen.findByText("No agents on the connected daemons.")).toBeTruthy();
  });

  it("shows a status error instead of the summary, and the summary again after it clears", async () => {
    const fake = fakeBackend(sampleView());
    render(<App backend={fake.backend} />);
    await screen.findByRole("heading", { name: "Pi Studio Dashboard" });
    const failed = sampleView();
    failed.ui.status = "bad params: `count` is too big";
    act(() => fake.emit(failed));
    expect(screen.getByRole("status").textContent).toBe("bad params: `count` is too big");
    act(() => fake.emit(sampleView()));
    expect(screen.getByRole("status").textContent).toContain("0 of 4 agents");
  });

  it("follows ui.theme on the document element, and leaves it unset before the first view", async () => {
    const fake = fakeBackend(sampleView());
    render(<App backend={fake.backend} />);
    expect(document.documentElement.dataset.theme).toBeUndefined();
    await screen.findByRole("heading", { name: "Pi Studio Dashboard" });
    expect(document.documentElement.dataset.theme).toBe("dark");
    const light = sampleView();
    light.ui.theme = "light";
    act(() => fake.emit(light));
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("resolves ui.theme system with the OS appearance and re-applies when it changes, without a command", async () => {
    const listeners = new Set<(event: MediaQueryListEvent) => void>();
    const query = {
      matches: false,
      addEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => listeners.add(listener),
      removeEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => listeners.delete(listener),
    };
    vi.stubGlobal("matchMedia", () => query);
    try {
      const system = sampleView();
      system.ui.theme = "system";
      const fake = fakeBackend(system);
      render(<App backend={fake.backend} />);
      await screen.findByRole("heading", { name: "Pi Studio Dashboard" });
      expect(document.documentElement.dataset.theme).toBe("light");
      act(() => {
        for (const listener of listeners) {
          listener({ matches: true } as MediaQueryListEvent);
        }
      });
      expect(document.documentElement.dataset.theme).toBe("dark");
      expect(fake.calls).toEqual([]);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("turns a shortcut into the id of its command and changes nothing locally", async () => {
    const fake = fakeBackend(sampleView());
    render(<App backend={fake.backend} />);
    await screen.findByRole("heading", { name: "Pi Studio Dashboard" });
    await userEvent.keyboard("{Control>}{Shift>}a{/Shift}{/Control}");
    expect(fake.calls).toEqual([["invoke", "view.toggleAttentionRail"]]);
    expect(screen.getByLabelText("Attention rail").className).not.toContain("collapsed");
  });

  it("hides empty menus and sends a bound id exactly as given", async () => {
    const fake = fakeBackend(sampleView());
    render(<App backend={fake.backend} />);
    await screen.findByRole("heading", { name: "Pi Studio Dashboard" });
    expect(screen.queryByRole("button", { name: "Daemon" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Agent" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "View" }));
    await userEvent.click(screen.getByRole("menuitemcheckbox", { name: /Theme: Light/ }));
    expect(fake.calls).toEqual([["invoke", "view.setTheme:light"]]);
  });

  it("says so when there is no app behind the page", async () => {
    render(<App backend={fakeBackend(null).backend} />);
    expect(await screen.findByText("no view")).toBeTruthy();
  });
});
