import { clip } from "../lib/format";
import type { GroupHeader } from "../lib/types";
import { StatusBadge } from "./StatusBadge";
import styles from "./WorkspaceHeader.module.css";

/**
 * The title of one section of the grouped grid: the workspace label, its full path, the daemon it
 * is on (only when cards from several daemons are shown, since one daemon is already in the
 * status bar), a count and a warning when an agent in it needs the engineer. A landmark, not a
 * button. Everything is plain text from the daemon, clipped here as a second guard.
 */
export function WorkspaceHeader({ group, showDaemon }: { group: GroupHeader; showDaemon: boolean }) {
  return (
    <div className={styles.header}>
      <span className={styles.label} title={group.path ?? group.label}>
        {clip(group.label, 80)}
      </span>
      {group.path !== null && group.path !== group.label && <span className={styles.path}>{clip(group.path, 200)}</span>}
      {showDaemon && <span className={styles.daemon}>{clip(group.daemonName, 80)}</span>}
      <span className={styles.badges}>
        {group.needsAttention && <StatusBadge tone="warning">needs attention</StatusBadge>}
        <StatusBadge tone="muted" title={`${group.count} agent(s)`}>
          {group.count}
        </StatusBadge>
      </span>
    </div>
  );
}
