import { clip } from "../lib/format";
import type { GroupHeader } from "../lib/types";
import { ChevronIcon } from "./icons";
import { StatusBadge } from "./StatusBadge";
import styles from "./WorkspaceHeader.module.css";

/**
 * The title of one section of the grouped grid, and the button that collapses or expands it: the
 * workspace label, its full path, the daemon it is on (only when cards from several daemons are
 * shown, since one daemon is already in the status bar), a count and a warning when an agent in it
 * needs the engineer (so a collapsed group never hides that). Everything is plain text from the
 * daemon, clipped here as a second guard.
 */
export function WorkspaceHeader({ group, showDaemon, onToggle }: { group: GroupHeader; showDaemon: boolean; onToggle: (key: string) => void }) {
  return (
    <button type="button" className={styles.header} aria-expanded={!group.collapsed} onClick={() => onToggle(group.key)}>
      <span className={styles.chevron}>
        <ChevronIcon open={!group.collapsed} />
      </span>
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
    </button>
  );
}
