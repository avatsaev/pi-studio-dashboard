import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { Banner } from "./Banner";

describe("Banner", () => {
  it("announces danger as an alert and warning/info as status", () => {
    const { rerender } = render(<Banner tone="danger">Authentication failed</Banner>);
    expect(screen.getByRole("alert").textContent).toContain("Authentication failed");
    rerender(<Banner tone="warning">Degraded mode</Banner>);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByRole("status").textContent).toContain("Degraded mode");
    rerender(<Banner tone="info">FYI</Banner>);
    expect(screen.getByRole("status").textContent).toContain("FYI");
  });

  it("has no action button unless given one, and runs it on click", async () => {
    const { rerender } = render(<Banner tone="warning">Duplicate entry</Banner>);
    expect(screen.queryByRole("button")).toBeNull();
    const onClick = vi.fn<() => void>();
    rerender(
      <Banner tone="danger" action={{ label: "Enter password", onClick }}>
        Auth failed
      </Banner>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Enter password" }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
