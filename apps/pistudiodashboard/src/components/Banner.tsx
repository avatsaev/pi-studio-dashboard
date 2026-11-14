import type { ReactNode } from "react";

import styles from "./Banner.module.css";
import { Button } from "./Button";
import { AlertIcon } from "./icons";

export type BannerTone = "warning" | "danger" | "info";

/** A full-width strip for a degraded or failed daemon: tinted fill, a 2 px edge bar, an optional action. */
export function Banner({ tone, children, action }: { tone: BannerTone; children: ReactNode; action?: { label: string; onClick: () => void } }) {
  return (
    <div className={`${styles.banner} ${styles[tone]}`} role={tone === "danger" ? "alert" : "status"}>
      <span className={styles.icon}>
        <AlertIcon />
      </span>
      <span className={styles.text}>{children}</span>
      {action && <Button onClick={action.onClick}>{action.label}</Button>}
    </div>
  );
}
