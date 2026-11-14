import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { emptyView, sampleView } from "../test-support";
import { StatusBar } from "./StatusBar";

describe("StatusBar", () => {
  it("shows the connected daemons and the totals label from the view", () => {
    render(<StatusBar document={sampleView().document} status="" />);
    expect(screen.getByRole("status").textContent).toBe("1 of 2 daemons connected · 0 of 4 agents");
  });

  it("reads zero on an empty monitor and 'Ready' without a document", () => {
    const { rerender } = render(<StatusBar document={emptyView().document} status="" />);
    expect(screen.getByRole("status").textContent).toBe("0 of 0 daemons connected · 0 of 0 agents");
    rerender(<StatusBar document={null} status="" />);
    expect(screen.getByRole("status").textContent).toBe("Ready");
  });

  it("lets an error replace the summary", () => {
    render(<StatusBar document={sampleView().document} status="boom" />);
    expect(screen.getByRole("status").textContent).toBe("boom");
  });
});
