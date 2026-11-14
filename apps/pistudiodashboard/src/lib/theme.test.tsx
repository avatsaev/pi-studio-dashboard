import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { applyTheme, DARK_QUERY, resolveTheme, useTheme } from "./theme";
import type { Theme } from "./types";

/** A `matchMedia` double: `flip` plays the OS changing appearance. */
function fakeMedia(initial: boolean) {
  let matches = initial;
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const queries: string[] = [];
  const list = {
    get matches() {
      return matches;
    },
    addEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => listeners.add(listener),
    removeEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => listeners.delete(listener),
  } as unknown as MediaQueryList;
  return {
    matchMedia: (query: string) => {
      queries.push(query);
      return list;
    },
    queries,
    listeners,
    flip: (next: boolean) => {
      matches = next;
      for (const listener of listeners) {
        listener({ matches: next } as MediaQueryListEvent);
      }
    },
  };
}

afterEach(() => {
  delete document.documentElement.dataset.theme;
  vi.unstubAllGlobals();
});

describe("resolveTheme", () => {
  it.each<[Theme, boolean, "dark" | "light"]>([
    ["dark", true, "dark"],
    ["dark", false, "dark"],
    ["light", true, "light"],
    ["light", false, "light"],
    ["system", true, "dark"],
    ["system", false, "light"],
  ])("%s with prefersDark=%s is %s", (choice, prefersDark, expected) => {
    expect(resolveTheme(choice, prefersDark)).toBe(expected);
  });
});

describe("applyTheme", () => {
  it("writes the choice and never asks the media query for dark or light", () => {
    const media = fakeMedia(false);
    const stop = applyTheme("dark", document, media.matchMedia);
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(media.queries).toEqual([]);
    stop();
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("follows the OS live under system and removes its one listener on cleanup", () => {
    const media = fakeMedia(true);
    const stop = applyTheme("system", document, media.matchMedia);
    expect(media.queries).toEqual([DARK_QUERY]);
    expect(media.listeners.size).toBe(1);
    expect(document.documentElement.dataset.theme).toBe("dark");
    media.flip(false);
    expect(document.documentElement.dataset.theme).toBe("light");
    media.flip(true);
    expect(document.documentElement.dataset.theme).toBe("dark");
    stop();
    expect(media.listeners.size).toBe(0);
  });
});

describe("useTheme", () => {
  it("writes nothing before the first view", () => {
    const media = fakeMedia(true);
    vi.stubGlobal("matchMedia", media.matchMedia);
    renderHook(() => useTheme(undefined));
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(media.listeners.size).toBe(0);
  });

  it("tracks the choice, follows the OS only under system, and drops the listener when the choice changes", () => {
    const media = fakeMedia(false);
    vi.stubGlobal("matchMedia", media.matchMedia);
    const { rerender, unmount } = renderHook(({ choice }: { choice: Theme }) => useTheme(choice), { initialProps: { choice: "light" as Theme } });
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(media.listeners.size).toBe(0);

    rerender({ choice: "system" });
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(media.listeners.size).toBe(1);
    media.flip(true);
    expect(document.documentElement.dataset.theme).toBe("dark");

    rerender({ choice: "dark" });
    expect(media.listeners.size).toBe(0);
    media.flip(false);
    expect(document.documentElement.dataset.theme).toBe("dark");

    rerender({ choice: "system" });
    expect(media.listeners.size).toBe(1);
    unmount();
    expect(media.listeners.size).toBe(0);
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});
