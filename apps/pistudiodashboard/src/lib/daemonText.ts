// Wording for daemon chips (daemon-connections § Error messages, § Connection states). Pure text built
// from what the view says; no connection logic: which state a daemon is in is decided in Rust.

import type { DaemonChip } from "./types";

/** Whole seconds to show for a retry delay: never "0s" while a retry is still pending. */
export function retrySeconds(ms: number | null): number | null {
  return ms === null ? null : Math.max(1, Math.ceil(ms / 1000));
}

function address(chip: DaemonChip): string {
  const host = chip.host ?? chip.name;
  return chip.port === null ? host : `${host}:${chip.port}`;
}

function retryTail(chip: DaemonChip): string {
  const seconds = retrySeconds(chip.nextRetryInMs);
  return seconds === null ? "" : ` Retrying in ${seconds}s.`;
}

/** The inline explanation of a failed or waiting chip, or null when there is nothing to explain. */
export function chipMessage(chip: DaemonChip): string | null {
  switch (chip.conn) {
    case "auth_failed":
      return chip.keychainUnavailable
        ? "Couldn't read the saved password — enter it to connect (it won't be saved)."
        : `Authentication failed for ${chip.name}. Check the password.`;
    case "host_rejected": {
      const host = chip.host ?? chip.name;
      return `${host} is not allowed by the daemon. Add ${host} to PI_STUDIO_HOSTNAMES on the daemon, or connect by IP address.`;
    }
    case "backoff":
      switch (chip.lastError?.code) {
        case "bad_handshake":
        case "handshake_rejected":
          return `${address(chip)} didn't answer like a pi-studio daemon.${retryTail(chip)}`;
        case "frame_too_large":
          return `Connection reset: oversized message from daemon.${retryTail(chip)}`;
        case "closed":
          return `Connection to ${address(chip)} closed.${retryTail(chip)}`;
        default:
          return `Can't reach ${address(chip)}.${retryTail(chip)}`;
      }
    default:
      return null;
  }
}

/** The tooltip of a connected chip: `<hostname> · pi-studio <version>`, each part only when known. */
export function connectedTooltip(chip: DaemonChip): string | null {
  const parts = [chip.hostname, chip.version ? `pi-studio ${chip.version}` : null].filter((p): p is string => Boolean(p));
  return parts.length > 0 ? parts.join(" · ") : null;
}

export const UNENCRYPTED_HINT = "Password and agent data travel unencrypted. Use wss:// if the daemon supports it.";
export const NO_OBSERVABILITY_HINT = "Health and logs need a newer pi-studio daemon.";

export function duplicateHint(chip: DaemonChip, chips: DaemonChip[]): string {
  const other = chips.find((c) => c.entryId === chip.duplicateOf)?.name ?? "another entry";
  return `Duplicate entry — same daemon as ${other}. This entry's connection is closed; remove or edit one of them to connect it.`;
}

/** Epoch ms as the user's local date and time. */
export function localTime(ms: number | null): string {
  return ms === null ? "—" : new Date(ms).toLocaleString();
}
