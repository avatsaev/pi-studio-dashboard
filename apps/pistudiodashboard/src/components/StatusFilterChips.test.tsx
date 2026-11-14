import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { StatusChip } from "../lib/types";
import { StatusFilterChips } from "./StatusFilterChips";

const chips: StatusChip[] = [
  { bucket: "running", count: 2, selected: false },
  { bucket: "error", count: 1, selected: true },
  { bucket: "idle", count: 3, selected: false },
];

describe("StatusFilterChips", () => {
  it("draws a chip per bucket with its count and the pressed state the view gave", () => {
    render(<StatusFilterChips chips={chips} filtered onChange={() => {}} onClear={() => {}} />);
    expect(screen.getByRole("button", { name: /running/ }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByRole("button", { name: /error/ }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: /idle/ }).textContent).toContain("3");
  });

  it("sends the new selection: adding a bucket, and removing a selected one", async () => {
    const onChange = vi.fn<(s: string[]) => void>();
    render(<StatusFilterChips chips={chips} filtered onChange={onChange} onClear={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /running/ }));
    expect(onChange).toHaveBeenLastCalledWith(["error", "running"]);
    await userEvent.click(screen.getByRole("button", { name: /error/ }));
    expect(onChange).toHaveBeenLastCalledWith([]);
  });

  it("offers 'Clear filters' only while a filter is on, and draws nothing with no chips and no filter", async () => {
    const onClear = vi.fn<() => void>();
    const { rerender, container } = render(<StatusFilterChips chips={chips} filtered={false} onChange={() => {}} onClear={onClear} />);
    expect(screen.queryByRole("button", { name: "Clear filters" })).toBeNull();
    rerender(<StatusFilterChips chips={chips} filtered onChange={() => {}} onClear={onClear} />);
    await userEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(onClear).toHaveBeenCalledTimes(1);
    rerender(<StatusFilterChips chips={[]} filtered={false} onChange={() => {}} onClear={onClear} />);
    expect(container.firstChild).toBeNull();
  });
});
