import type { ButtonHTMLAttributes } from "react";

import styles from "./Button.module.css";

export type ButtonVariant = "secondary" | "primary" | "destructive";

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> {
  variant?: ButtonVariant;
  /** Disables the button and says why in the tooltip. */
  disabledReason?: string;
}

/** The minimal button a dialog footer or banner needs; every colour comes from tokens. */
export function Button({ variant = "secondary", disabledReason, disabled, title, type = "button", ...rest }: ButtonProps) {
  return <button {...rest} type={type} className={`${styles.button} ${styles[variant]}`} disabled={disabled || disabledReason !== undefined} title={disabledReason ?? title} />;
}
