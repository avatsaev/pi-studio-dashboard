import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { sampleView } from "../test-support";
import { MenuBar } from "./MenuBar";

function bar(onInvoke = vi.fn<(id: string) => void>()) {
  render(<MenuBar menus={sampleView().menus} mac={false} onInvoke={onInvoke} />);
  return onInvoke;
}

describe("MenuBar", () => {
  it("draws only menus that have items and reports the chosen id unchanged", async () => {
    const onInvoke = bar();
    expect(screen.queryByRole("button", { name: "Daemon" })).toBeNull();
    expect(screen.queryByRole("menu")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "View" }));
    const rail = screen.getByRole("menuitem", { name: /Toggle Attention Rail/ });
    expect(rail.textContent).toContain("Ctrl+Shift+A");
    await userEvent.click(screen.getByRole("menuitemcheckbox", { name: /Theme: Light/ }));
    expect(onInvoke).toHaveBeenCalledWith("view.setTheme:light");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closes on a click elsewhere and on Escape", async () => {
    bar();
    await userEvent.click(screen.getByRole("button", { name: "View" }));
    expect(screen.getByRole("menu")).toBeTruthy();
    await userEvent.click(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "View" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("disables what cannot run now and says why", async () => {
    const menus = sampleView().menus;
    const rail = menus[2].items[3];
    rail.enabled = false;
    rail.reason = "nothing to toggle";
    const onInvoke = vi.fn<(id: string) => void>();
    render(<MenuBar menus={menus} mac={false} onInvoke={onInvoke} />);
    await userEvent.click(screen.getByRole("button", { name: "View" }));
    const entry = screen.getByRole("menuitem", { name: /Toggle Attention Rail/ });
    expect(entry).toHaveProperty("disabled", true);
    expect(entry.getAttribute("title")).toBe("nothing to toggle");
    await userEvent.click(entry);
    expect(onInvoke).not.toHaveBeenCalled();
  });

  it("shows the checked theme entry", async () => {
    bar();
    await userEvent.click(screen.getByRole("button", { name: "View" }));
    expect(screen.getByRole("menuitemcheckbox", { name: /Theme: Dark/ }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("menuitemcheckbox", { name: /Theme: System/ }).getAttribute("aria-checked")).toBe("false");
  });
});
