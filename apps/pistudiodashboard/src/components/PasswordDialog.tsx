import { useId, useState, type FormEvent } from "react";

import { execute, fieldErrors, setDialog } from "../lib/commands";
import type { Backend, DaemonChip } from "../lib/types";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import { CheckField, TextField } from "./Field";
import styles from "./DaemonDialogs.module.css";

const KEYCHAIN_NOTE = "Couldn't read the saved password — enter it to connect (it won't be saved).";

/** `UiState.dialog.kind == "password"`: write-only. The typed text stays here and goes into `daemon.setPassword` (D18, D32). */
export function PasswordDialog({ backend, entry }: { backend: Backend; entry: DaemonChip }) {
  const formId = useId();
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(!entry.keychainUnavailable);
  const [error, setError] = useState<string | undefined>();
  const [busy, setBusy] = useState(false);
  const close = () => void setDialog(backend, null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(undefined);
    const reply = await execute(backend, "daemon.setPassword", { entryId: entry.entryId, password, remember });
    if (!reply.ok) {
      const parsed = fieldErrors(reply.error);
      setError(parsed.fields.password ?? parsed.rest ?? Object.values(parsed.fields)[0]);
      setBusy(false);
      return;
    }
    close();
  }

  return (
    <Dialog
      title="Enter password"
      onClose={close}
      footer={
        <>
          <Button onClick={close}>Cancel</Button>
          <Button variant="primary" type="submit" form={formId} disabled={busy || password === ""}>
            Connect
          </Button>
        </>
      }
    >
      <form id={formId} className={styles.form} onSubmit={(event) => void submit(event)}>
        <p className={styles.paragraph}>
          Password for <span className={styles.name}>{entry.name}</span>
        </p>
        {entry.keychainUnavailable && (
          <p className={styles.note} role="note">
            {KEYCHAIN_NOTE}
          </p>
        )}
        <TextField label="Password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} error={error} autoComplete="current-password" />
        <CheckField label="Remember in the system keychain" checked={remember} onChange={setRemember} />
      </form>
    </Dialog>
  );
}
