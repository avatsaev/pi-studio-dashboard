import styles from "./EmptyState.module.css";
import { Button } from "./Button";

/** A centred placeholder: a title, an optional detail line and an optional action. No icon. */
export function EmptyState({ title, detail, action }: { title: string; detail?: string; action?: { label: string; onClick: () => void } }) {
  return (
    <div className={styles.empty}>
      <p className={styles.title}>{title}</p>
      {detail && <p className={styles.detail}>{detail}</p>}
      {action && <Button onClick={action.onClick}>{action.label}</Button>}
    </div>
  );
}
