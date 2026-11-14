import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef, useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { Button } from "./Button";
import { Dialog } from "./Dialog";

function Harness({ onClose = () => {}, safe = false }: { onClose?: () => void; safe?: boolean }) {
  const [open, setOpen] = useState(false);
  const cancel = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open
      </button>
      {open && (
        <Dialog
          title="Remove daemon"
          initialFocus={safe ? cancel : undefined}
          onClose={() => {
            onClose();
            setOpen(false);
          }}
          footer={
            <>
              <button ref={cancel} type="button" onClick={() => setOpen(false)}>
                Cancel
              </button>
              <Button variant="destructive">Remove</Button>
            </>
          }
        >
          <input type="text" aria-label="Name" />
        </Dialog>
      )}
    </>
  );
}

async function open(props: { onClose?: () => void; safe?: boolean } = {}) {
  render(<Harness {...props} />);
  await userEvent.click(screen.getByRole("button", { name: "Open" }));
  return screen.getByRole("dialog", { name: "Remove daemon" });
}

describe("Dialog", () => {
  it("is a modal dialog labelled by its title", async () => {
    const dialog = await open();
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(screen.getByRole("heading", { name: "Remove daemon" })).toBeTruthy();
  });

  it("starts on the first field, or on the safe button when told", async () => {
    await open();
    expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Name" }));
  });

  it("starts on the safe button (initialFocus) instead of the field", async () => {
    await open({ safe: true });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }));
  });

  it("traps Tab and Shift+Tab inside", async () => {
    await open();
    const [name, cancel, remove] = [screen.getByRole("textbox", { name: "Name" }), screen.getByRole("button", { name: "Cancel" }), screen.getByRole("button", { name: "Remove" })];
    expect(document.activeElement).toBe(name);
    await userEvent.tab();
    expect(document.activeElement).toBe(cancel);
    await userEvent.tab();
    expect(document.activeElement).toBe(remove);
    await userEvent.tab();
    expect(document.activeElement).toBe(name);
    await userEvent.tab({ shift: true });
    expect(document.activeElement).toBe(remove);
    expect(screen.getByRole("button", { name: "Open" })).not.toBe(document.activeElement);
  });

  it("closes on Escape", async () => {
    const onClose = vi.fn<() => void>();
    await open({ onClose });
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes on a click on the scrim, not on a click inside the panel", async () => {
    const onClose = vi.fn<() => void>();
    const dialog = await open({ onClose });
    await userEvent.click(dialog);
    await userEvent.click(screen.getByRole("textbox", { name: "Name" }));
    expect(onClose).not.toHaveBeenCalled();
    const scrim = dialog.parentElement;
    expect(scrim).not.toBeNull();
    if (scrim) {
      await userEvent.click(scrim);
    }
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("returns focus to the opener after closing", async () => {
    await open();
    await userEvent.keyboard("{Escape}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Open" }));
  });
});
