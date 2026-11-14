import type { MonitorView } from "../lib/types";
import styles from "./Header.module.css";

/** "N of M daemons connected", from the chips the shell sent. */
export function connectedText(document: MonitorView | null): string {
  const chips = document?.daemonChips ?? [];
  return `${chips.filter((chip) => chip.conn === "live").length} of ${chips.length} daemons connected`;
}

export function Header({ document }: { document: MonitorView | null }) {
  return (
    <header className={styles.header}>
      <h1>Pi Studio Dashboard</h1>
      <span className={styles.connected}>{connectedText(document)}</span>
    </header>
  );
}
