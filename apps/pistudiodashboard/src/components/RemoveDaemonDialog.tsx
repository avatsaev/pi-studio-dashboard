import { useState } from "react";

import { execute, setDialog } from "../lib/commands";
import type { Backend, DaemonChip } from "../lib/types";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import styles from "./DaemonDialogs.module.css";

/** `UiState.dialog.kind == "removeDaemon"`: names the daemon and says there is no undo. Cancel, the first control, has the focus. */
export function RemoveDaemonDialog({ backend, entry }: { backend: Backend; entry: DaemonChip }) {
  const [failure, setFailure] = useState<string | null>(null);
  const close = () => void setDialog(backend, null);

  async function remove() {
    const reply = await execute(backend, "daemon.remove", { entryId: entry.entryId });
    if (reply.ok) {
      close();
    } else {
      setFailure(reply.error);
    }
  }

  return (
    <Dialog
      title="Remove daemon?"
      onClose={close}
      footer={
        <>
          <Button onClick={close}>Cancel</Button>
          <Button variant="destructive" onClick={() => void remove()}>
            Remove
          </Button>
        </>
      }
    >
      <p className={styles.paragraph}>
        Remove <span className={styles.name}>{entry.name}</span> from the dashboard? Its agents disappear from the grid and its saved password is deleted. There is no undo.
      </p>
      {failure && (
        <p className={styles.failure} role="alert">
          {failure}
        </p>
      )}
    </Dialog>
  );
}
