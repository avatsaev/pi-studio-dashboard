import type { Backend, DaemonChip, Dialog } from "../lib/types";
import { DaemonFormDialog } from "./DaemonFormDialog";
import { PasswordDialog } from "./PasswordDialog";
import { RemoveDaemonDialog } from "./RemoveDaemonDialog";

/** Draws the daemon dialog `UiState.dialog` names, if it is one (the interrupt confirm belongs to the agent UI). */
export function DaemonDialogs({ dialog, chips, backend }: { dialog: Dialog | null; chips: DaemonChip[]; backend: Backend }) {
  if (!dialog) {
    return null;
  }
  const entryOf = (id: string | null | undefined) => chips.find((chip) => chip.entryId === id) ?? null;
  switch (dialog.kind) {
    case "daemonForm": {
      const entry = entryOf(dialog.entryId);
      return dialog.mode === "edit" && !entry ? null : <DaemonFormDialog key={`${dialog.mode}:${dialog.entryId ?? ""}`} backend={backend} mode={dialog.mode} entry={entry} />;
    }
    case "password": {
      const entry = entryOf(dialog.entryId);
      return entry && <PasswordDialog key={dialog.entryId} backend={backend} entry={entry} />;
    }
    case "removeDaemon": {
      const entry = entryOf(dialog.entryId);
      return entry && <RemoveDaemonDialog key={dialog.entryId} backend={backend} entry={entry} />;
    }
    default:
      return null;
  }
}
