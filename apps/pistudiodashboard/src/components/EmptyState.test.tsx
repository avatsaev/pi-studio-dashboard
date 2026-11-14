import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("shows a title alone", () => {
    render(<EmptyState title="No daemons yet. Add one from the Daemon menu." />);
    expect(screen.getByText("No daemons yet. Add one from the Daemon menu.")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("shows a detail line and an action that is run on click", async () => {
    const onClick = vi.fn<() => void>();
    render(<EmptyState title="Nothing here" detail="Try again later" action={{ label: "Add daemon", onClick }} />);
    expect(screen.getByText("Try again later")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Add daemon" }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
