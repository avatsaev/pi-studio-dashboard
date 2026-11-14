import styles from "./AttentionRail.module.css";

// Empty in this task; attention items arrive with sprint-007.
export function AttentionRail({ collapsed }: { collapsed: boolean }) {
  return (
    <aside className={collapsed ? `${styles.rail} ${styles.collapsed}` : styles.rail} aria-label="Attention rail">
      {!collapsed && <h2>Needs attention</h2>}
    </aside>
  );
}
