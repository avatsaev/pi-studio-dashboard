import { useId, useState, type FormEvent } from "react";

import { execute, fieldErrors, setDialog } from "../lib/commands";
import { UNENCRYPTED_HINT } from "../lib/daemonText";
import type { Backend, DaemonChip } from "../lib/types";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import { CheckField, TextField } from "./Field";
import styles from "./DaemonDialogs.module.css";

const DEFAULT_PORT = "6767";

/** Only to decide whether to show the plaintext hint while typing; whether the entry is valid is the command's call. */
function isLoopbackHost(host: string): boolean {
  const h = host.trim().toLowerCase().replace(/^\[|\]$/g, "");
  return h === "localhost" || h === "::1" || /^127(\.\d{1,3}){3}$/.test(h);
}

function newEntryId(result: unknown): string | null {
  return result && typeof result === "object" && "entryId" in result && typeof result.entryId === "string" ? result.entryId : null;
}

/**
 * `UiState.dialog.kind == "daemonForm"`. The typed text, the password included, lives only in this
 * component and goes straight into a command: nothing here reaches a view value (D18, D32).
 */
export function DaemonFormDialog({ backend, mode, entry }: { backend: Backend; mode: "add" | "edit"; entry: DaemonChip | null }) {
  const formId = useId();
  const [name, setName] = useState(entry?.name ?? "");
  const [host, setHost] = useState(entry?.host ?? "");
  const [port, setPort] = useState(entry?.port == null ? DEFAULT_PORT : String(entry.port));
  const [tls, setTls] = useState(entry?.tls ?? false);
  const [webUiUrl, setWebUiUrl] = useState(entry?.webUiUrl ?? "");
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(true);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const close = () => void setDialog(backend, null);
  const editing = mode === "edit";

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setErrors({});
    setFailure(null);
    const base = { name, host, port: Number(port), tls };
    const reply = editing
      ? await execute(backend, "daemon.edit", { ...base, entryId: entry?.entryId, webUiUrl: webUiUrl.trim() === "" ? null : webUiUrl })
      : await execute(backend, "daemon.add", {
          ...base,
          ...(webUiUrl.trim() === "" ? {} : { webUiUrl }),
          // `daemon.add` always remembers a password it is given; one that must not be saved goes in afterwards.
          ...(password !== "" && remember ? { password } : {}),
        });
    if (!reply.ok) {
      const parsed = fieldErrors(reply.error);
      setErrors(parsed.fields);
      setFailure(parsed.rest);
      setBusy(false);
      return;
    }
    const entryId = editing ? (entry?.entryId ?? null) : newEntryId(reply.result);
    if (password !== "" && entryId && (editing || !remember)) {
      const stored = await execute(backend, "daemon.setPassword", { entryId, password, remember });
      if (!stored.ok) {
        const parsed = fieldErrors(stored.error);
        setErrors(parsed.fields);
        setFailure(parsed.rest);
        setBusy(false);
        return;
      }
    }
    close();
  }

  return (
    <Dialog
      title={editing ? "Edit daemon" : "Add daemon"}
      onClose={close}
      footer={
        <>
          <Button onClick={close}>Cancel</Button>
          <Button variant="primary" type="submit" form={formId} disabled={busy}>
            {editing ? "Save" : "Add"}
          </Button>
        </>
      }
    >
      <form id={formId} className={styles.form} onSubmit={(event) => void submit(event)} noValidate>
        <TextField label="Name" value={name} onChange={(e) => setName(e.target.value)} error={errors.name} autoComplete="off" />
        <div className={styles.row}>
          <TextField label="Host" value={host} onChange={(e) => setHost(e.target.value)} error={errors.host} placeholder="127.0.0.1" autoComplete="off" spellCheck={false} />
          <div className={styles.port}>
            <TextField label="Port" value={port} onChange={(e) => setPort(e.target.value)} error={errors.port} inputMode="numeric" autoComplete="off" />
          </div>
        </div>
        <CheckField label="Use TLS (wss://)" checked={tls} onChange={setTls} />
        {!tls && host.trim() !== "" && !isLoopbackHost(host) && (
          <p className={styles.warning} role="note">
            Plain ws:// to a non-loopback host. {UNENCRYPTED_HINT}
          </p>
        )}
        <TextField label="Web UI URL" value={webUiUrl} onChange={(e) => setWebUiUrl(e.target.value)} error={errors.webUiUrl} placeholder="http://localhost:4173" autoComplete="off" spellCheck={false} hint="Optional. Used to open an agent in the pi-studio web UI." />
        <TextField
          label="Password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          error={errors.password}
          autoComplete="new-password"
          hint={editing ? "Leave empty to keep the current password." : "Optional. Never shown again."}
        />
        <CheckField label="Remember in the system keychain" checked={remember} onChange={setRemember} />
        {errors.daemons && (
          <p className={styles.failure} role="alert">
            {errors.daemons}
          </p>
        )}
        {failure && (
          <p className={styles.failure} role="alert">
            {failure}
          </p>
        )}
      </form>
    </Dialog>
  );
}
