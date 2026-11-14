import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { FilterChip } from "./FilterChip";

describe("FilterChip", () => {
  it("reports a toggle and shows the pressed state it was given", async () => {
    const onToggle = vi.fn<() => void>();
    const { rerender } = render(<FilterChip label="running" count={3} active={false} onToggle={onToggle} />);
    const chip = screen.getByRole("button", { name: /running/ });
    expect(chip.getAttribute("aria-pressed")).toBe("false");
    await userEvent.click(chip);
    expect(onToggle).toHaveBeenCalledTimes(1);
    // The parent owns the state: nothing changes until it re-renders with the view's answer.
    expect(chip.getAttribute("aria-pressed")).toBe("false");
    rerender(<FilterChip label="running" count={3} active onToggle={onToggle} />);
    expect(screen.getByRole("button", { name: /running/ }).getAttribute("aria-pressed")).toBe("true");
  });

  it("draws the count and a leading status dot for status chips", () => {
    render(<FilterChip label="error" count={2} active={false} status="error" onToggle={() => {}} />);
    expect(screen.getByText("2")).toBeTruthy();
    expect(screen.getByRole("img", { name: "error" })).toBeTruthy();
  });

  it("omits the count and dot when not given", () => {
    render(<FilterChip label="workstation" active={false} onToggle={() => {}} />);
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByRole("button").textContent).toBe("workstation");
  });

  it("is operable from the keyboard", async () => {
    const onToggle = vi.fn<() => void>();
    render(<FilterChip label="idle" active={false} onToggle={onToggle} />);
    await userEvent.tab();
    await userEvent.keyboard("{Enter}");
    expect(onToggle).toHaveBeenCalledTimes(1);
  });
});
