import { useEffect, useState } from "react";

import { execute, setDialog } from "../lib/commands";
import { chipMessage, connectedTooltip, duplicateHint, NO_OBSERVABILITY_HINT, retrySeconds } from "../lib/daemonText";
import type { Backend, DaemonChip, Menu } from "../lib/types";
import { Button } from "./Button";
import { ConnectionPill } from "./ConnectionPill";
import styles from "./DaemonChips.module.css";
import { AlertIcon } from "./icons";

type Glyph = "duplicate" | "observability";

const CONNECTED = ["live", "probing", "connecting", "handshaking", "syncing"];

function glyphText(kind: Glyph, chip: DaemonChip, chips: DaemonChip[]): string {
  return kind === "duplicate" ? duplicateHint(chip, chips) : NO_OBSERVABILITY_HINT;
}

function menuReason(chip: DaemonChip, command: "connect" | "disconnect"): string | undefined {
  if (command === "connect") {
    return CONNECTED.includes(chip.conn) ? "Already connected or connecting" : undefined;
  }
  return chip.conn === "idle" || chip.conn === "disabled" ? "Already disconnected" : undefined;
}

/**
 * The daemon chip row: one chip per daemon with its connection state (label and dot, never colour
 * alone), the failure explained below, and a menu of daemon actions. It only draws `daemonChips` and
 * sends command ids back: which state a daemon is in, and why, is decided in Rust.
 */
export function DaemonChips({ chips, menus, backend }: { chips: DaemonChip[]; menus: Menu[]; backend: Backend }) {
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [explained, setExplained] = useState<{ entryId: string; kind: Glyph } | null>(null);

  useEffect(() => {
    if (!menuFor) {
      return;
    }
    const close = () => setMenuFor(null);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        close();
      }
    };
    const onPointer = (event: MouseEvent) => {
      if (!(event.target instanceof Element) || !event.target.closest("[data-chip-menu]")) {
        close();
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onPointer);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onPointer);
    };
  }, [menuFor]);

  if (chips.length === 0) {
    const detect = menus.flatMap((m) => m.items).find((item) => item.id === "daemon.detectLocal");
    return (
      <section className={styles.empty} aria-label="Daemons">
        <span>No daemons configured. Running pi-studio on this machine?</span>
        <Button onClick={() => void backend.invoke("daemon.detectLocal")} disabledReason={detect?.enabled ? undefined : (detect?.reason ?? "Local daemon detection is not available")}>
          Detect local daemon
        </Button>
      </section>
    );
  }

  const toggle = (chip: DaemonChip) => {
    const ids = chips.filter((c) => (c.entryId === chip.entryId ? !c.selected : c.selected)).flatMap((c) => (c.serverId ? [c.serverId] : []));
    void execute(backend, "view.filterDaemons", { serverIds: ids });
  };
  const run = (action: () => Promise<unknown>) => {
    setMenuFor(null);
    void action();
  };
  const withEntry = (chip: DaemonChip) => ({ entryId: chip.entryId });
  const messages = chips.flatMap((chip) => {
    const text = chipMessage(chip);
    return text ? [{ chip, text }] : [];
  });
  const explainedChip = explained ? chips.find((c) => c.entryId === explained.entryId) : undefined;

  return (
    <div className={styles.area}>
      <ul className={styles.chips} aria-label="Daemons">
        {chips.map((chip) => {
          const glyphs: Glyph[] = [...(chip.duplicateOf ? (["duplicate"] as const) : []), ...(chip.degraded.includes("no_observability") ? (["observability"] as const) : [])];
          const tooltip = chip.conn === "live" ? (connectedTooltip(chip) ?? chip.name) : chip.name;
          return (
            <li key={chip.entryId} className={chip.selected ? `${styles.chip} ${styles.selected}` : styles.chip} data-conn={chip.conn} data-chip-menu>
              <button
                type="button"
                className={styles.main}
                aria-pressed={chip.selected}
                disabled={chip.serverId === null}
                title={chip.serverId === null ? `${chip.name}: can be selected once the daemon has connected` : tooltip}
                onClick={() => toggle(chip)}
              >
                <span className={styles.name}>{chip.name}</span>
                <span className={styles.count} title={`${chip.agentCount} agents`}>
                  {chip.agentCount}
                </span>
              </button>
              <ConnectionPill conn={chip.conn} wasLive={chip.staleSince !== null} retryInSeconds={retrySeconds(chip.nextRetryInMs)} unencrypted={chip.unencrypted} />
              {glyphs.map((kind) => (
                <button
                  key={kind}
                  type="button"
                  className={styles.glyph}
                  aria-label={kind === "duplicate" ? `Duplicate entry: ${chip.name}` : `No observability: ${chip.name}`}
                  aria-expanded={explained?.entryId === chip.entryId && explained.kind === kind}
                  title={glyphText(kind, chip, chips)}
                  onClick={() => setExplained((cur) => (cur?.entryId === chip.entryId && cur.kind === kind ? null : { entryId: chip.entryId, kind }))}
                >
                  <AlertIcon size={12} />
                </button>
              ))}
              {chip.conn === "auth_failed" && (
                <button type="button" className={styles.action} onClick={() => void setDialog(backend, { kind: "password", entryId: chip.entryId })}>
                  Enter password
                </button>
              )}
              <button
                type="button"
                className={styles.more}
                aria-label={`Menu for ${chip.name}`}
                aria-haspopup="menu"
                aria-expanded={menuFor === chip.entryId}
                onClick={() => setMenuFor((cur) => (cur === chip.entryId ? null : chip.entryId))}
              >
                ⋯
              </button>
              {menuFor === chip.entryId && (
                <ul className={styles.menu} role="menu" aria-label={`${chip.name} actions`}>
                  <MenuItem label="Edit…" onSelect={() => run(() => setDialog(backend, { kind: "daemonForm", mode: "edit", entryId: chip.entryId }))} />
                  <MenuItem label="Set password…" onSelect={() => run(() => setDialog(backend, { kind: "password", entryId: chip.entryId }))} />
                  <MenuItem label="Connect" reason={menuReason(chip, "connect")} onSelect={() => run(() => execute(backend, "daemon.connect", withEntry(chip)))} />
                  <MenuItem label="Disconnect" reason={menuReason(chip, "disconnect")} onSelect={() => run(() => execute(backend, "daemon.disconnect", withEntry(chip)))} />
                  <MenuItem label="Diagnostics" onSelect={() => run(() => execute(backend, "daemon.showDiagnostics", withEntry(chip)))} />
                  <MenuItem label="Remove…" danger onSelect={() => run(() => setDialog(backend, { kind: "removeDaemon", entryId: chip.entryId }))} />
                </ul>
              )}
            </li>
          );
        })}
      </ul>
      {messages.map(({ chip, text }) => (
        <p key={chip.entryId} className={styles.message} data-tone={chip.conn === "backoff" ? "warning" : "danger"} role={chip.conn === "backoff" ? undefined : "alert"}>
          <strong className={styles.bannerName}>{chip.name}</strong> {text}
        </p>
      ))}
      {explained && explainedChip && (
        <p className={styles.message} data-tone="info">
          <strong className={styles.bannerName}>{explainedChip.name}</strong> {glyphText(explained.kind, explainedChip, chips)}
        </p>
      )}
    </div>
  );
}

function MenuItem({ label, onSelect, reason, danger }: { label: string; onSelect: () => void; reason?: string; danger?: boolean }) {
  return (
    <li role="none">
      <button type="button" role="menuitem" className={danger ? `${styles.item} ${styles.danger}` : styles.item} disabled={reason !== undefined} title={reason} onClick={onSelect}>
        {label}
      </button>
    </li>
  );
}
