// Theme application. The view carries the user's choice (`dark | light | system`); the resolved
// value is display only, so the webview computes it: `matchMedia` is the one reliable source of the
// OS appearance in both WebKitGTK and WKWebView, and nothing in the app depends on the result.
// Before the first view nothing is written, so `tokens.css` shows its default (dark) with no flash.

import { useEffect } from "react";

import type { Theme } from "./types";

export const DARK_QUERY = "(prefers-color-scheme: dark)";

export type Resolved = "dark" | "light";

/** `system` follows the OS; anything else is itself. */
export function resolveTheme(choice: Theme, prefersDark: boolean): Resolved {
  if (choice === "system") {
    return prefersDark ? "dark" : "light";
  }
  return choice;
}

/**
 * Write `data-theme` on the document element. For `system` it also registers one `change` listener
 * on the media query that re-resolves live. Returns the cleanup: it removes that listener and
 * clears `data-theme`.
 */
export function applyTheme(choice: Theme, doc: Document, matchMedia: (query: string) => MediaQueryList): () => void {
  const root = doc.documentElement;
  if (choice !== "system") {
    root.dataset.theme = resolveTheme(choice, false);
    return () => {
      delete root.dataset.theme;
    };
  }
  const query = matchMedia(DARK_QUERY);
  root.dataset.theme = resolveTheme(choice, query.matches);
  const onChange = (event: MediaQueryListEvent) => {
    root.dataset.theme = resolveTheme(choice, event.matches);
  };
  query.addEventListener("change", onChange);
  return () => {
    query.removeEventListener("change", onChange);
    delete root.dataset.theme;
  };
}

/** Keep `<html data-theme>` in step with the view's choice. `undefined` (no view yet) writes nothing. */
export function useTheme(choice: Theme | undefined): void {
  useEffect(() => {
    if (choice === undefined) {
      return;
    }
    return applyTheme(choice, document, (query) => window.matchMedia(query));
  }, [choice]);
}
