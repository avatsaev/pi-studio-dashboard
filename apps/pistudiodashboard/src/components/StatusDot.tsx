import type { AgentStatus } from "../lib/types";
import styles from "./StatusDot.module.css";

/** Why a dot is overridden: a pending dialog (waiting for the user) or an errored agent. */
export type Attention = "dialog" | "error" | null;

export type DotTone = "accent" | "accentBright" | "muted" | "warning" | "danger";

export interface DotStyle {
  /** `ring` = spinner (work in progress), `flat` = filled dot, `outline` = unknown. */
  shape: "ring" | "flat" | "outline";
  tone: DotTone;
  /** A soft pulse around the dot: only for a pending dialog. */
  pulse: boolean;
  /** The status word the dot stands for: it is never colour-only. */
  label: string;
}

const KNOWN: ReadonlySet<string> = new Set<AgentStatus>(["initializing", "idle", "running", "error", "closed"]);

/**
 * The dashboard's single status -> dot translation (design-system § Status mapping; D15, D26).
 * Precedence: pending dialog > errored > base status. An unknown status (a newer daemon) draws an
 * outlined muted dot labelled "unknown"; it never throws.
 */
export function dotStyle(status: string, attention: Attention = null): DotStyle {
  const word = KNOWN.has(status) ? status : "unknown";
  if (attention === "dialog") {
    return { shape: "flat", tone: "warning", pulse: true, label: `${word}, needs a response` };
  }
  if (attention === "error") {
    return { shape: "flat", tone: "danger", pulse: false, label: word === "error" ? word : `${word}, errored` };
  }
  switch (word) {
    case "running":
      return { shape: "ring", tone: "accentBright", pulse: false, label: word };
    case "initializing":
      return { shape: "flat", tone: "accent", pulse: false, label: word };
    case "error":
      return { shape: "flat", tone: "danger", pulse: false, label: word };
    case "idle":
    case "closed":
      return { shape: "flat", tone: "muted", pulse: false, label: word };
    default:
      return { shape: "outline", tone: "muted", pulse: false, label: word };
  }
}

export function StatusDot({ status, attention = null, label }: { status: AgentStatus; attention?: Attention; label?: string }) {
  const dot = dotStyle(status, attention);
  const text = label ?? dot.label;
  const classes = [dot.shape === "ring" ? styles.ring : styles.dot, styles[dot.tone]];
  if (dot.shape === "outline") {
    classes.push(styles.outline);
  }
  if (dot.pulse) {
    classes.push(styles.pulse);
  }
  return (
    // A CSS-drawn dot has no image source to put in an <img>.
    // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role
    <span className={styles.slot} role="img" aria-label={text} title={text} data-shape={dot.shape} data-tone={dot.tone}>
      <span className={classes.join(" ")} />
    </span>
  );
}
