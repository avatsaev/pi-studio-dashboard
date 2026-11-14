import { UNENCRYPTED_HINT } from "../lib/daemonText";
import { StatusBadge, type BadgeTone } from "./StatusBadge";
import styles from "./ConnectionPill.module.css";

export interface PillSpec {
  tone: BadgeTone;
  label: string;
  /** Work in progress: draw a spinner instead of a flat dot. */
  spinner: boolean;
}

/**
 * Connection state -> tone and label (daemon-connections § Connection states shown on daemon chips,
 * design-system § Connection pill). The only place a connection state becomes a colour.
 *
 * - probing / connecting / handshaking / syncing: accent spinner, "Connecting…"
 * - live: success, "Connected"
 * - backoff: danger "Unreachable" when the daemon was never live since start; warning "Reconnecting ·
 *   next try in Ns" after having been live ("Reconnecting…" while no retry time is known)
 * - auth_failed / host_rejected: danger
 * - idle / disabled: muted
 * - anything else (a newer client's state): muted "Unknown"; it never throws
 */
export function pillSpec(conn: string, opts: { wasLive?: boolean; retryInSeconds?: number | null } = {}): PillSpec {
  switch (conn) {
    case "probing":
    case "connecting":
    case "handshaking":
    case "syncing":
      return { tone: "accent", label: "Connecting…", spinner: true };
    case "live":
      return { tone: "success", label: "Connected", spinner: false };
    case "backoff":
      if (!opts.wasLive) {
        return { tone: "danger", label: "Unreachable", spinner: false };
      }
      return { tone: "warning", label: opts.retryInSeconds == null ? "Reconnecting…" : `Reconnecting · next try in ${opts.retryInSeconds}s`, spinner: false };
    case "auth_failed":
      return { tone: "danger", label: "Auth failed", spinner: false };
    case "host_rejected":
      return { tone: "danger", label: "Host not allowed", spinner: false };
    case "idle":
      return { tone: "muted", label: "Disconnected", spinner: false };
    case "disabled":
      return { tone: "muted", label: "Disabled", spinner: false };
    default:
      return { tone: "muted", label: "Unknown", spinner: false };
  }
}

/** The badge recipe plus a state mark; `unencrypted` adds the warning badge beside it (D32). */
export function ConnectionPill({
  conn,
  wasLive,
  retryInSeconds,
  unencrypted = false,
}: {
  conn: string;
  /** The daemon has been live since the app started (`staleSince` is set on the chip). */
  wasLive?: boolean;
  retryInSeconds?: number | null;
  unencrypted?: boolean;
}) {
  const spec = pillSpec(conn, { wasLive, retryInSeconds });
  return (
    <>
      <StatusBadge tone={spec.tone}>
        <span className={spec.spinner ? styles.spinner : styles.mark} aria-hidden="true" />
        {spec.label}
      </StatusBadge>
      {unencrypted && (
        <StatusBadge tone="warning" title={UNENCRYPTED_HINT}>
          unencrypted
        </StatusBadge>
      )}
    </>
  );
}
