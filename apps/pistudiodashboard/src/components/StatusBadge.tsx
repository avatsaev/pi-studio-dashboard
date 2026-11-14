import type { ReactNode } from "react";

import styles from "./StatusBadge.module.css";

export type BadgeTone = "success" | "danger" | "warning" | "info" | "accent" | "muted";

/** A tinted pill: daemon version, "unencrypted", degraded flags. Colour is the tone, the words are the content. */
export function StatusBadge({ tone, children, title }: { tone: BadgeTone; children: ReactNode; title?: string }) {
  const hint = title ?? (typeof children === "string" ? children : undefined);
  return (
    <span className={`${styles.badge} ${styles[tone]}`} title={hint}>
      {children}
    </span>
  );
}
