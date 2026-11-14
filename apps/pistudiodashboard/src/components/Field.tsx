import { useId, type InputHTMLAttributes, type ReactNode } from "react";

import styles from "./Field.module.css";

/** A labelled text input with an optional error line (first error of that field) and hint, wired with aria. */
export function TextField({
  label,
  error,
  hint,
  ...input
}: { label: string; error?: string; hint?: ReactNode } & Omit<InputHTMLAttributes<HTMLInputElement>, "className" | "id" | "aria-invalid">) {
  const id = useId();
  const messageId = `${id}-message`;
  return (
    <div className={styles.field}>
      <label htmlFor={id} className={styles.label}>
        {label}
      </label>
      <input {...input} id={id} className={error ? `${styles.input} ${styles.invalid}` : styles.input} aria-invalid={error ? true : undefined} aria-describedby={error || hint ? messageId : undefined} />
      {error ? (
        <p id={messageId} className={styles.error} role="alert">
          {error}
        </p>
      ) : (
        hint && (
          <p id={messageId} className={styles.hint}>
            {hint}
          </p>
        )
      )}
    </div>
  );
}

export function CheckField({ label, checked, onChange, hint }: { label: string; checked: boolean; onChange: (checked: boolean) => void; hint?: string }) {
  const id = useId();
  return (
    <div className={styles.check}>
      <input id={id} type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
      <label htmlFor={id}>{label}</label>
      {hint && <span className={styles.hint}>{hint}</span>}
    </div>
  );
}
