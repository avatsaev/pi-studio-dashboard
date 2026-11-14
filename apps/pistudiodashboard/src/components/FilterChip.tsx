import type { AgentStatus } from "../lib/types";
import styles from "./FilterChip.module.css";
import { StatusDot } from "./StatusDot";

/**
 * A toggle chip (daemon or status filter). It only draws and reports: dispatching the filter command
 * is the parent's job, and the view says whether it is active.
 */
export function FilterChip({
  label,
  count,
  active,
  status,
  title,
  onToggle,
}: {
  label: string;
  count?: number;
  active: boolean;
  /** A leading status dot (status chips). */
  status?: AgentStatus;
  title?: string;
  onToggle: () => void;
}) {
  return (
    <button type="button" className={active ? `${styles.chip} ${styles.active}` : styles.chip} aria-pressed={active} title={title ?? label} onClick={onToggle}>
      {status && <StatusDot status={status} />}
      <span className={styles.label}>{label}</span>
      {count !== undefined && <span className={styles.count}>{count}</span>}
    </button>
  );
}
