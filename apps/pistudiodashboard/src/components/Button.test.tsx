import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { Button } from "./Button";

describe("Button", () => {
  it("is a plain button that runs its handler", async () => {
    const onClick = vi.fn<() => void>();
    render(<Button onClick={onClick}>Cancel</Button>);
    const button = screen.getByRole("button", { name: "Cancel" });
    expect(button.getAttribute("type")).toBe("button");
    await userEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("draws every variant", () => {
    render(
      <>
        <Button variant="secondary">a</Button>
        <Button variant="primary">b</Button>
        <Button variant="destructive">c</Button>
      </>,
    );
    expect(screen.getAllByRole("button")).toHaveLength(3);
  });

  it("is disabled with the reason as its tooltip", async () => {
    const onClick = vi.fn<() => void>();
    render(
      <Button onClick={onClick} disabledReason="nothing selected">
        Interrupt
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Interrupt" });
    expect(button).toHaveProperty("disabled", true);
    expect(button.getAttribute("title")).toBe("nothing selected");
    await userEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });
});
