import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Toast } from "../lib/types";
import { reducedMotionCss } from "../test-support";
import { TOAST_ERROR_MS, TOAST_EXIT_MS, TOAST_MS, ToastViewport } from "./ToastViewport";

const info = (id: number, text = `toast ${id}`): Toast => ({ id, kind: "info", text });

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

function tick(ms: number) {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

/** Run the clock to when a toast starts leaving, then through its exit transition (React renders in between). */
function expire(ms: number) {
  tick(ms);
  tick(TOAST_EXIT_MS);
}

describe("ToastViewport", () => {
  it("shows ui.toasts and hides each one after its timer, with no backend involved", () => {
    render(<ToastViewport toasts={[info(1)]} />);
    expect(screen.getByText("toast 1")).toBeTruthy();
    tick(TOAST_MS - 1);
    expect(screen.getByText("toast 1")).toBeTruthy();
    tick(1);
    // Leaving: still in the DOM for the exit transition, then gone.
    tick(TOAST_EXIT_MS);
    expect(screen.queryByText("toast 1")).toBeNull();
  });

  it("keeps an error longer than an info", () => {
    render(<ToastViewport toasts={[info(1), { id: 2, kind: "error", text: "it broke" }]} />);
    expire(TOAST_MS);
    expect(screen.queryByText("toast 1")).toBeNull();
    expect(screen.getByText("it broke")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("it broke");
    expire(TOAST_ERROR_MS - TOAST_MS - TOAST_EXIT_MS);
    expect(screen.queryByText("it broke")).toBeNull();
  });

  it("does not bring an expired toast back when the shell still lists it, but shows a new id", () => {
    const { rerender } = render(<ToastViewport toasts={[info(1)]} />);
    expire(TOAST_MS);
    rerender(<ToastViewport toasts={[info(1)]} />);
    expect(screen.queryByText("toast 1")).toBeNull();
    rerender(<ToastViewport toasts={[info(1), info(2)]} />);
    expect(screen.queryByText("toast 1")).toBeNull();
    expect(screen.getByText("toast 2")).toBeTruthy();
  });

  it("gives a later toast its own timer", () => {
    const { rerender } = render(<ToastViewport toasts={[info(1)]} />);
    tick(4000);
    rerender(<ToastViewport toasts={[info(1), info(2)]} />);
    expire(2000);
    expect(screen.queryByText("toast 1")).toBeNull();
    expect(screen.getByText("toast 2")).toBeTruthy();
    // Toast 2 started at 4 s, so it leaves at 10 s.
    expire(10000 - 4000 - 2000 - TOAST_EXIT_MS);
    expect(screen.queryByText("toast 2")).toBeNull();
  });

  it("can be dismissed early, locally", async () => {
    vi.useRealTimers();
    render(<ToastViewport toasts={[info(1)]} />);
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    await vi.waitFor(() => expect(screen.queryByText("toast 1")).toBeNull());
  });

  it("ellipsises long text and keeps the full text in title", () => {
    const text = "x".repeat(300);
    render(<ToastViewport toasts={[info(1, text)]} />);
    expect(screen.getByText(text).getAttribute("title")).toBe(text);
  });

  it("draws nothing for no toasts, and drops timers on unmount", () => {
    const { container, unmount } = render(<ToastViewport toasts={[]} />);
    expect(container.querySelectorAll("button")).toHaveLength(0);
    unmount();
    const other = render(<ToastViewport toasts={[info(9)]} />);
    other.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("does not animate under prefers-reduced-motion", () => {
    expect(reducedMotionCss("./components/ToastViewport.module.css")).toMatch(/\.toast[\s\S]*transition:\s*none/);
  });
});
