// Display-only formatting of values the shell sends as data. The shell has no clock and no locale,
// so "12.3k", "as of 14:02" and "stale since 14:02" are made here, from the local clock (D28).

/** A usage older than this is shown "as of <time>" instead of as live (running agents refresh every 10 s). */
export const USAGE_FRESH_MS = 30_000;

/** Hard limit on the characters of daemon text put in the page, whatever the shell capped. */
export const MAX_TEXT = 400;

/** The text cut to `max` characters with an ellipsis. Plain text only: it is never markup. */
export function clip(text: string, max: number = MAX_TEXT): string {
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

const UNITS: readonly [number, string][] = [
  [1e9, "B"],
  [1e6, "M"],
  [1e3, "k"],
];

/** 999 -> "999", 12_345 -> "12.3k", 1_250_000 -> "1.3M". */
export function compactNumber(n: number): string {
  if (!Number.isFinite(n)) {
    return "—";
  }
  const sign = n < 0 ? "-" : "";
  const abs = Math.abs(n);
  for (const [size, unit] of UNITS) {
    if (abs >= size) {
      const scaled = abs / size;
      const text = scaled >= 100 ? scaled.toFixed(0) : scaled.toFixed(1);
      // 999.96k would round to "1000k": that is "1M".
      if (Number(text) >= 1000 && unit !== "B") {
        return `${sign}1${UNITS[UNITS.findIndex(([, u]) => u === unit) - 1]?.[1] ?? unit}`;
      }
      return `${sign}${text.replace(/\.0$/, "")}${unit}`;
    }
  }
  return `${sign}${Math.round(abs)}`;
}

/** The daemon's cost exactly as sent: a bare number, no currency (the stats payload has none). */
export function formatCost(cost: number): string {
  // 12 significant digits drop the float noise of a sum (0.1 + 0.2) and keep any real precision.
  return String(Number(cost.toPrecision(12)));
}

/** Local time of day, with the date when it is not today. `""` for a value that is not a time. */
export function formatClock(ms: number, now: number = Date.now()): string {
  const at = new Date(ms);
  if (Number.isNaN(at.getTime())) {
    return "";
  }
  const time = at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return at.toDateString() === new Date(now).toDateString() ? time : `${at.toLocaleDateString([], { month: "short", day: "numeric" })} ${time}`;
}
