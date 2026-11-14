import { useCallback, useEffect, useState } from "react";

import type { Toast } from "../lib/types";
import { CloseIcon } from "./icons";
import styles from "./ToastViewport.module.css";

/** How long a toast stays; an error stays longer. A frontend timer: nothing is sent back to the shell. */
export const TOAST_MS = 6000;
export const TOAST_ERROR_MS = 12000;
/** The exit transition (CSS, 180 ms); the toast leaves the DOM after it. */
export const TOAST_EXIT_MS = 180;

function ToastItem({ toast, onGone }: { toast: Toast; onGone: (id: number) => void }) {
  const [leaving, setLeaving] = useState(false);
  const { id, kind } = toast;

  useEffect(() => {
    const timer = setTimeout(() => setLeaving(true), kind === "error" ? TOAST_ERROR_MS : TOAST_MS);
    return () => clearTimeout(timer);
  }, [id, kind]);

  useEffect(() => {
    if (!leaving) {
      return;
    }
    const timer = setTimeout(() => onGone(id), TOAST_EXIT_MS);
    return () => clearTimeout(timer);
  }, [leaving, id, onGone]);

  return (
    <div className={`${styles.toast} ${styles[kind]}${leaving ? ` ${styles.exiting}` : ""}`} role={kind === "error" ? "alert" : undefined}>
      <span className={styles.content} title={toast.text}>
        {toast.text}
      </span>
      <button type="button" className={styles.close} aria-label="Dismiss" onClick={() => setLeaving(true)}>
        <CloseIcon size={12} />
      </button>
    </div>
  );
}

/**
 * Draws `ui.toasts`. The shell keeps a toast listed until it is told otherwise; whether one is still on
 * screen is this component's own business, kept per toast `id` (a toast with a new id shows again).
 */
export function ToastViewport({ toasts }: { toasts: Toast[] }) {
  const [gone, setGone] = useState<ReadonlySet<number>>(new Set());
  const onGone = useCallback((id: number) => setGone((current) => new Set(current).add(id)), []);

  // Forget ids the shell no longer lists (adjusting state while rendering, the documented pattern).
  const listed = new Set(toasts.map((toast) => toast.id));
  if ([...gone].some((id) => !listed.has(id))) {
    setGone(new Set([...gone].filter((id) => listed.has(id))));
  }

  return (
    <div className={styles.viewport} aria-live="polite">
      {toasts
        .filter((toast) => !gone.has(toast.id))
        .map((toast) => (
          <ToastItem key={toast.id} toast={toast} onGone={onGone} />
        ))}
    </div>
  );
}
