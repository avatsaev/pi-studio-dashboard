import { describe, expect, it } from "vitest";

import { findShortcut, formatShortcut, matches } from "./shortcuts";
import type { Menu, Shortcut } from "./types";

const undo: Shortcut = { text: "Cmd+Z", cmd: true, ctrl: false, shift: false, alt: false, key: "z" };
const redo: Shortcut = { ...undo, text: "Cmd+Shift+Z", shift: true };
const entry = (id: string, shortcut: Shortcut) => ({ id, label: id, shortcut, enabled: true, reason: null, checked: null });
const menus: Menu[] = [{ label: "Edit", items: [entry("edit.undo", undo), entry("edit.redo", redo)] }];

function key(init: KeyboardEventInit, target: EventTarget = document.body): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { bubbles: true, ...init });
  target.dispatchEvent(event);
  return event;
}

describe("shortcuts", () => {
  it("treats Cmd as Ctrl off macOS and as the command key on it", () => {
    expect(matches(undo, key({ key: "z", ctrlKey: true }), false)).toBe(true);
    expect(matches(undo, key({ key: "z", metaKey: true }), false)).toBe(false);
    expect(matches(undo, key({ key: "z", metaKey: true }), true)).toBe(true);
    expect(matches(undo, key({ key: "z", ctrlKey: true }), true)).toBe(false);
  });

  it("matches modifiers exactly, so redo never runs undo", () => {
    const pressed = key({ key: "Z", ctrlKey: true, shiftKey: true });
    expect(matches(undo, pressed, false)).toBe(false);
    expect(matches(redo, pressed, false)).toBe(true);
    expect(findShortcut(menus, pressed, false)).toBe("edit.redo");
    expect(findShortcut(menus, key({ key: "q", ctrlKey: true }), false)).toBeNull();
  });

  it("leaves undo to a text field that has the focus", () => {
    const field = document.createElement("input");
    document.body.append(field);
    expect(findShortcut(menus, key({ key: "z", ctrlKey: true }, field), false)).toBeNull();
    expect(findShortcut(menus, key({ key: "z", ctrlKey: true }), false)).toBe("edit.undo");
    field.remove();
  });

  it("does nothing for the shortcut of a disabled command (Esc with no drill-down open)", () => {
    const escape: Shortcut = { text: "Escape", cmd: false, ctrl: false, shift: false, alt: false, key: "escape" };
    const closed: Menu[] = [{ label: "Agent", items: [{ ...entry("agent.deselect", escape), enabled: false, reason: "no agent details are open" }] }];
    const open: Menu[] = [{ label: "Agent", items: [entry("agent.deselect", escape)] }];
    expect(findShortcut(closed, key({ key: "Escape" }), false)).toBeNull();
    expect(findShortcut(open, key({ key: "Escape" }), false)).toBe("agent.deselect");
  });

  it("formats for the platform", () => {
    expect(formatShortcut(redo, false)).toBe("Ctrl+Shift+Z");
    expect(formatShortcut(redo, true)).toBe("⇧⌘Z");
    expect(formatShortcut({ ...undo, cmd: false, alt: true, key: "f4" }, false)).toBe("Alt+F4");
  });
});
