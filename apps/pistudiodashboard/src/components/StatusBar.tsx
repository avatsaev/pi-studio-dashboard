import { compactNumber, formatCost } from "../lib/format";
import type { MonitorView } from "../lib/types";
import { connectedText } from "./Header";
import styles from "./StatusBar.module.css";

// Text comes from the view (`totals.label` is built in Rust); the only counting here is the
// connected-daemon words, which are the chips' own states. Tokens and cost are partial sums over
// the agents that have usage: they are shown only next to the "N of M agents" label that says so.
function summary(document: MonitorView | null): string {
  if (!document) {
    return "Ready";
  }
  const { totals } = document;
  const parts = [connectedText(document), totals.label];
  if (totals.covered > 0) {
    parts.push(`${compactNumber(totals.tokens)} tokens`, `cost ${formatCost(totals.cost)}`);
  }
  return parts.join(" · ");
}

export function StatusBar({ document, status }: { document: MonitorView | null; status: string }) {
  const text = status || summary(document);
  return (
    <output className={styles.bar}>
      <span className={status ? styles.error : styles.summary} title={text}>
        {text}
      </span>
    </output>
  );
}
