import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";

import styles from "./Dialog.module.css";

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
const FIELDS = "input:not([disabled]), select:not([disabled]), textarea:not([disabled])";

/**
 * A modal shell: scrim, titled panel, footer slot. Content and forms are the caller's. Focus is trapped
 * inside, starts on `initialFocus` (e.g. the safe button) or else the first field, and returns to the
 * opener on close. `Esc` and a click on the scrim call `onClose`; the caller decides what that sends
 * (typically `ui.set {dialog: null}`).
 */
export function Dialog({
  title,
  onClose,
  children,
  footer,
  initialFocus,
  wide = false,
}: {
  title: string;
  onClose: () => void;
  children?: ReactNode;
  /** Right-aligned buttons. */
  footer?: ReactNode;
  initialFocus?: RefObject<HTMLElement | null>;
  /** A wider panel for read-only detail such as diagnostics. */
  wide?: boolean;
}) {
  const titleId = useId();
  const panel = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const root = panel.current;
    const first = root?.querySelector<HTMLElement>(FIELDS) ?? root?.querySelector<HTMLElement>(FOCUSABLE);
    (initialFocus?.current ?? first ?? root)?.focus();
    return () => {
      if (opener?.isConnected) {
        opener.focus();
      }
    };
  }, [initialFocus]);

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "Tab" || !panel.current) {
      return;
    }
    const focusable = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = focusable.at(0);
    const last = focusable.at(-1);
    if (!first || !last) {
      event.preventDefault();
    } else if (event.shiftKey && (document.activeElement === first || document.activeElement === panel.current)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  return createPortal(
    <div
      className={styles.scrim}
      role="presentation"
      onKeyDown={onKeyDown}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <dialog ref={panel} open className={wide ? `${styles.panel} ${styles.wide}` : styles.panel} aria-modal="true" aria-labelledby={titleId} tabIndex={-1}>
        <h2 id={titleId} className={styles.title}>
          {title}
        </h2>
        {children && <div className={styles.body}>{children}</div>}
        {footer && <div className={styles.footer}>{footer}</div>}
      </dialog>
    </div>,
    document.body,
  );
}
