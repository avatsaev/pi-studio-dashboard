import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { chip, fakeBackend, sampleView } from "../test-support";
import { DaemonDialogs } from "./DaemonDialogs";

const secret = "hunter2-s3cret";

// The fake backend does not redraw, so a finished submit is seen by the calls it made.
const settled = (fake: { calls: unknown[] }, count: number) => waitFor(() => expect(fake.calls.length).toBeGreaterThanOrEqual(count));

describe("DaemonFormDialog", () => {
  it("add: shows the command's field errors (first per field) and stays open", async () => {
    const fake = fakeBackend(null, () => ({
      ok: false,
      error: `bad params: ${JSON.stringify({ errors: [{ field: "host", code: "invalid_host", message: "host is required" }, { field: "host", code: "x", message: "second" }, { field: "port", code: "invalid_port", message: "port must be 1-65535" }, { field: "name", code: "duplicate_name", message: "name already used" }] })}`,
    }));
    render(<DaemonDialogs dialog={{ kind: "daemonForm", mode: "add" }} chips={[]} backend={fake.backend} />);
    await userEvent.type(screen.getByLabelText("Name"), "x");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByText("host is required")).toBeTruthy();
    expect(screen.queryByText("second")).toBeNull();
    expect(screen.getByText("port must be 1-65535")).toBeTruthy();
    expect(screen.getByText("name already used")).toBeTruthy();
    expect(fake.calls.some((c) => c[0] === "control" && c[1] === "ui.set")).toBe(false);
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("add: submits daemon.add with the typed values and closes; the default port is 6767", async () => {
    const fake = fakeBackend(null, () => ({ ok: true, result: { entryId: "new" } }));
    render(<DaemonDialogs dialog={{ kind: "daemonForm", mode: "add" }} chips={[]} backend={fake.backend} />);
    expect((screen.getByLabelText("Port") as HTMLInputElement).value).toBe("6767");
    await userEvent.type(screen.getByLabelText("Name"), "box");
    await userEvent.type(screen.getByLabelText("Host"), "10.0.0.5");
    await userEvent.type(screen.getByLabelText("Password"), secret);
    expect(screen.getByRole("note").textContent).toContain("unencrypted");
    expect((screen.getByLabelText("Password") as HTMLInputElement).type).toBe("password");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    await settled(fake, 2);
    expect(fake.calls[0]).toEqual(["control", "engine.execute", { command: "daemon.add", params: { name: "box", host: "10.0.0.5", port: 6767, tls: false, password: secret } }]);
    expect(fake.calls[1]).toEqual(["control", "ui.set", { dialog: null }]);
  });

  it("add without 'remember' stores the password afterwards through daemon.setPassword", async () => {
    const fake = fakeBackend(null, () => ({ ok: true, result: { entryId: "new" } }));
    render(<DaemonDialogs dialog={{ kind: "daemonForm", mode: "add" }} chips={[]} backend={fake.backend} />);
    await userEvent.type(screen.getByLabelText("Name"), "box");
    await userEvent.type(screen.getByLabelText("Host"), "localhost");
    await userEvent.type(screen.getByLabelText("Password"), secret);
    await userEvent.click(screen.getByLabelText("Remember in the system keychain"));
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    await settled(fake, 2);
    expect(fake.calls[0]?.[2]).toEqual({ command: "daemon.add", params: { name: "box", host: "localhost", port: 6767, tls: false } });
    expect(fake.calls[1]?.[2]).toEqual({ command: "daemon.setPassword", params: { entryId: "new", password: secret, remember: false } });
  });

  it("edit: prefilled, an empty password leaves it unchanged, a typed one goes to daemon.setPassword", async () => {
    const fake = fakeBackend(null);
    const entry = chip("e1", "box", { host: "h", port: 7000, webUiUrl: "http://x" });
    const { unmount } = render(<DaemonDialogs dialog={{ kind: "daemonForm", mode: "edit", entryId: "e1" }} chips={[entry]} backend={fake.backend} />);
    expect((screen.getByLabelText("Host") as HTMLInputElement).value).toBe("h");
    expect((screen.getByLabelText("Password") as HTMLInputElement).value).toBe("");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await settled(fake, 2);
    expect(fake.calls.map((c) => (c[0] === "control" ? c[1] : c[0]))).toEqual(["engine.execute", "ui.set"]);
    unmount();
    const second = fakeBackend(null);
    render(<DaemonDialogs dialog={{ kind: "daemonForm", mode: "edit", entryId: "e1" }} chips={[entry]} backend={second.backend} />);
    await userEvent.type(screen.getByLabelText("Password"), secret);
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await settled(second, 2);
    expect(second.calls[1]?.[2]).toEqual({ command: "daemon.setPassword", params: { entryId: "e1", password: secret, remember: true } });
  });
});

describe("PasswordDialog", () => {
  it("submits daemon.setPassword, never puts the password in a view value, and shows the keychain note when flagged", async () => {
    const fake = fakeBackend(null);
    const view = sampleView();
    const entry = chip("e1", "box", { conn: "auth_failed", keychainUnavailable: true });
    render(<DaemonDialogs dialog={{ kind: "password", entryId: "e1" }} chips={[entry]} backend={fake.backend} />);
    expect(screen.getByRole("note").textContent).toContain("Couldn't read the saved password");
    await userEvent.type(screen.getByLabelText("Password"), secret);
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    await settled(fake, 2);
    expect(fake.calls[0]).toEqual(["control", "engine.execute", { command: "daemon.setPassword", params: { entryId: "e1", password: secret, remember: false } }]);
    expect(JSON.stringify(view)).not.toContain(secret);
    expect(JSON.stringify(entry)).not.toContain(secret);
  });

  it("shows a command error under the field and stays open", async () => {
    const fake = fakeBackend(null, () => ({ ok: false, error: `bad params: {"errors":[{"field":"password","code":"invalid_password","message":"password is too long"}]}` }));
    render(<DaemonDialogs dialog={{ kind: "password", entryId: "e1" }} chips={[chip("e1", "box")]} backend={fake.backend} />);
    await userEvent.type(screen.getByLabelText("Password"), "x");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(await screen.findByText("password is too long")).toBeTruthy();
  });
});

describe("RemoveDaemonDialog", () => {
  it("names the daemon and says there is no undo; confirm runs daemon.remove, cancel runs nothing", async () => {
    const fake = fakeBackend(null);
    const { unmount } = render(<DaemonDialogs dialog={{ kind: "removeDaemon", entryId: "e1" }} chips={[chip("e1", "box")]} backend={fake.backend} />);
    expect(screen.getByText(/There is no undo/)).toBeTruthy();
    expect(screen.getByText("box")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(fake.calls).toEqual([["control", "ui.set", { dialog: null }]]);
    unmount();
    const second = fakeBackend(null);
    render(<DaemonDialogs dialog={{ kind: "removeDaemon", entryId: "e1" }} chips={[chip("e1", "box")]} backend={second.backend} />);
    await userEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(second.calls[0]).toEqual(["control", "engine.execute", { command: "daemon.remove", params: { entryId: "e1" } }]);
  });

  it("renders a hostile name as plain text", () => {
    const hostile = "<script>alert(1)</script>";
    const { baseElement } = render(<DaemonDialogs dialog={{ kind: "removeDaemon", entryId: "e1" }} chips={[chip("e1", hostile)]} backend={fakeBackend(null).backend} />);
    expect(baseElement.querySelector("script")).toBeNull();
    expect(screen.getByText(hostile)).toBeTruthy();
  });
});
