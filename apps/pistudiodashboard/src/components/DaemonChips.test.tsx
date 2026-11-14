import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { chipMessage } from "../lib/daemonText";
import type { DaemonChip } from "../lib/types";
import { chip, emptyView, fakeBackend } from "../test-support";
import { DaemonChips } from "./DaemonChips";

const menus = emptyView().menus;

function draw(chips: DaemonChip[], backend = fakeBackend(null).backend) {
  return render(<DaemonChips chips={chips} menus={menus} backend={backend} />);
}

describe("DaemonChips states", () => {
  it.each<[string, Partial<DaemonChip>, string, string]>([
    ["connecting", { conn: "connecting" }, "Connecting…", "accent"],
    ["connected", { conn: "live" }, "Connected", "success"],
    ["reconnecting", { conn: "backoff", staleSince: 1, nextRetryInMs: 3200 }, "Reconnecting · next try in 4s", "warning"],
    ["unreachable", { conn: "backoff", lastError: { code: "unreachable", message: null } }, "Unreachable", "danger"],
    ["auth_failed", { conn: "auth_failed" }, "Auth failed", "danger"],
    ["host_rejected", { conn: "host_rejected" }, "Host not allowed", "danger"],
    ["disconnected", { conn: "idle" }, "Disconnected", "muted"],
  ])("%s shows its label as text, with the state's tone", (_name, more, label, tone) => {
    draw([chip("e1", "box", more)]);
    const badge = screen.getByText(label).closest("span");
    expect(badge?.className).toContain(tone);
  });

  it("a connected chip's tooltip is hostname and version; a missing version is omitted", () => {
    draw([chip("e1", "a", { hostname: "devbox", version: "0.0.115" }), chip("e2", "b", { hostname: "other" })]);
    expect(screen.getByRole("button", { name: /^a/ }).title).toBe("devbox · pi-studio 0.0.115");
    expect(screen.getByRole("button", { name: /^b/ }).title).toBe("other");
  });

  it("explains each failure with the spec wording", () => {
    const c = (more: Partial<DaemonChip>) => chip("e1", "Box", { host: "10.0.0.9", port: 6790, ...more });
    expect(chipMessage(c({ conn: "backoff", lastError: { code: "unreachable", message: null }, nextRetryInMs: 2314 }))).toBe("Can't reach 10.0.0.9:6790. Retrying in 3s.");
    expect(chipMessage(c({ conn: "auth_failed" }))).toBe("Authentication failed for Box. Check the password.");
    expect(chipMessage(c({ conn: "auth_failed", keychainUnavailable: true }))).toBe("Couldn't read the saved password — enter it to connect (it won't be saved).");
    expect(chipMessage(c({ conn: "host_rejected" }))).toBe("10.0.0.9 is not allowed by the daemon. Add 10.0.0.9 to PI_STUDIO_HOSTNAMES on the daemon, or connect by IP address.");
    expect(chipMessage(c({ conn: "backoff", lastError: { code: "bad_handshake", message: null } }))).toBe("10.0.0.9:6790 didn't answer like a pi-studio daemon.");
    expect(chipMessage(c({ conn: "backoff", lastError: { code: "frame_too_large", message: null } }))).toBe("Connection reset: oversized message from daemon.");
    expect(chipMessage(c({ conn: "live" }))).toBeNull();
  });

  it("shows the 403 and unreachable messages in the row", () => {
    draw([
      chip("e1", "gate", { conn: "host_rejected", host: "pi.lan" }),
      chip("e2", "far", { conn: "backoff", lastError: { code: "unreachable", message: null }, nextRetryInMs: 5000, host: "10.0.0.9", port: 6799 }),
    ]);
    expect(screen.getByText(/pi\.lan is not allowed by the daemon.*PI_STUDIO_HOSTNAMES/)).toBeTruthy();
    expect(screen.getByText(/Can't reach 10\.0\.0\.9:6799\. Retrying in 5s\./)).toBeTruthy();
  });

  it("shows the unencrypted badge only when the view says so", () => {
    const { rerender } = draw([chip("e1", "a", { unencrypted: true })]);
    expect(screen.getByText("unencrypted")).toBeTruthy();
    rerender(<DaemonChips chips={[chip("e1", "a", { tls: true })]} menus={menus} backend={fakeBackend(null).backend} />);
    expect(screen.queryByText("unencrypted")).toBeNull();
  });

  it("draws a duplicate and a no-observability glyph that open their explanation", async () => {
    draw([chip("e1", "primary"), chip("e2", "second", { duplicateOf: "e1", degraded: ["no_observability"] })]);
    await userEvent.click(screen.getByRole("button", { name: "Duplicate entry: second" }));
    expect(screen.getByText(/same daemon as primary/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "No observability: second" }));
    expect(screen.getByText("Health and logs need a newer pi-studio daemon.", { exact: false })).toBeTruthy();
  });

  it("renders hostile and very long names as plain text", () => {
    const hostile = `<img src=x onerror=alert(1)>${"w".repeat(300)}`;
    const { container } = draw([chip("e1", hostile, { host: "<b>h</b>" })]);
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText(hostile, { selector: "span" })).toBeTruthy();
    expect(container.querySelector("b")).toBeNull();
  });
});

describe("DaemonChips actions", () => {
  it("toggles the daemon filter through view.filterDaemons", async () => {
    const fake = fakeBackend(null);
    draw([chip("e1", "a", { serverId: "s1", selected: true }), chip("e2", "b", { serverId: "s2" })], fake.backend);
    await userEvent.click(screen.getByRole("button", { name: /^b/ }));
    expect(fake.calls).toEqual([["control", "engine.execute", { command: "view.filterDaemons", params: { serverIds: ["s1", "s2"] } }]]);
  });

  it("a chip without a serverId cannot be selected", () => {
    draw([chip("e1", "a", { serverId: null, conn: "connecting" })]);
    expect((screen.getByRole("button", { name: /^a/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("'Enter password' on an auth_failed chip opens the password dialog via ui.set", async () => {
    const fake = fakeBackend(null);
    draw([chip("e1", "box", { conn: "auth_failed" })], fake.backend);
    await userEvent.click(screen.getByRole("button", { name: "Enter password" }));
    expect(fake.calls).toEqual([["control", "ui.set", { dialog: { kind: "password", entryId: "e1" } }]]);
  });

  it("the chip menu edits, removes, connects, disconnects and opens diagnostics with registry commands", async () => {
    const fake = fakeBackend(null);
    draw([chip("e1", "box", { conn: "backoff" })], fake.backend);
    const open = async () => userEvent.click(screen.getByRole("button", { name: "Menu for box" }));
    await open();
    await userEvent.click(screen.getByRole("menuitem", { name: "Edit…" }));
    await open();
    await userEvent.click(screen.getByRole("menuitem", { name: "Remove…" }));
    await open();
    await userEvent.click(screen.getByRole("menuitem", { name: "Connect" }));
    await open();
    await userEvent.click(screen.getByRole("menuitem", { name: "Disconnect" }));
    await open();
    await userEvent.click(screen.getByRole("menuitem", { name: "Diagnostics" }));
    expect(fake.calls).toEqual([
      ["control", "ui.set", { dialog: { kind: "daemonForm", mode: "edit", entryId: "e1" } }],
      ["control", "ui.set", { dialog: { kind: "removeDaemon", entryId: "e1" } }],
      ["control", "engine.execute", { command: "daemon.connect", params: { entryId: "e1" } }],
      ["control", "engine.execute", { command: "daemon.disconnect", params: { entryId: "e1" } }],
      ["control", "engine.execute", { command: "daemon.showDiagnostics", params: { entryId: "e1" } }],
    ]);
  });

  it("disables Connect on a live daemon and Disconnect on a disconnected one", async () => {
    draw([chip("e1", "live"), chip("e2", "off", { conn: "idle" })]);
    await userEvent.click(screen.getByRole("button", { name: "Menu for live" }));
    expect((within(screen.getByRole("menu")).getByRole("menuitem", { name: "Connect" }) as HTMLButtonElement).disabled).toBe(true);
    await userEvent.keyboard("{Escape}");
    await userEvent.click(screen.getByRole("button", { name: "Menu for off" }));
    expect((within(screen.getByRole("menu")).getByRole("menuitem", { name: "Disconnect" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("with no daemons, points to detecting the local one", async () => {
    const fake = fakeBackend(null);
    render(<DaemonChips chips={[]} menus={[{ label: "Daemon", items: [{ id: "daemon.detectLocal", label: "Detect", shortcut: null, enabled: true, reason: null, checked: null }] }]} backend={fake.backend} />);
    await userEvent.click(screen.getByRole("button", { name: "Detect local daemon" }));
    expect(fake.calls).toEqual([["invoke", "daemon.detectLocal"]]);
  });
});
