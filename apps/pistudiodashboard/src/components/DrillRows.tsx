import { clip } from "../lib/format";
import type { DrillItem } from "../lib/types";
import styles from "./DrillRows.module.css";
import { ToolKindIcon } from "./icons";

/** Characters of one text row put in the page; the model keeps what the daemon sent, the page never shows more. */
const MAX_ROW_TEXT = 20_000;
const MAX_LINE = 300;
/** A running tool shows this many lines of its live output. */
export const TAIL_LINES = 12;

const TURN_WORDS = { started: "turn started", completed: "turn completed", failed: "turn failed", canceled: "turn canceled" } as const;

/** The last line with something on it: what a finished call said, in one line. */
export function resultSummary(tail: string[] | null): string {
  const line = [...(tail ?? [])].reverse().find((l) => l.trim() !== "");
  return line ? clip(line.trim(), MAX_LINE) : "";
}

function ToolRow({ item }: { item: Extract<DrillItem, { type: "tool" }> }) {
  const running = item.status === "running" || item.status === "started";
  const tail = running ? (item.outputTail ?? []).slice(-TAIL_LINES) : [];
  const summary = running ? "" : resultSummary(item.outputTail);
  const tone = ["running", "started", "completed", "error", "canceled"].includes(item.status) ? item.status : "canceled";
  return (
    <div className={`${styles.tool} ${styles[tone]}`}>
      <div className={styles.toolHead}>
        <span className={styles.toolIcon} title={item.kind}>
          <ToolKindIcon kind={item.kind} />
        </span>
        <span className={styles.toolLabel} title={clip(item.label, MAX_LINE)}>
          {clip(item.label, MAX_LINE)}
        </span>
        <span className={styles.toolStatus}>{clip(item.status, 24)}</span>
      </div>
      {tail.length > 0 && <pre className={styles.output}>{tail.map((line) => clip(line, MAX_LINE)).join("\n")}</pre>}
      {summary && <div className={styles.summary}>{summary}</div>}
    </div>
  );
}

/** One timeline row. Everything is plain text (never markup, never markdown); long lines wrap. */
export function DrillRow({ item }: { item: DrillItem }) {
  switch (item.type) {
    case "assistant":
      return <p className={styles.assistant}>{clip(item.text, MAX_ROW_TEXT)}</p>;
    case "reasoning":
      return (
        <details className={styles.reasoning}>
          <summary>thinking</summary>
          <p>{clip(item.text, MAX_ROW_TEXT)}</p>
        </details>
      );
    case "tool":
      return <ToolRow item={item} />;
    case "turn":
      return <div className={item.kind === "failed" ? `${styles.turn} ${styles.turnFailed}` : styles.turn}>{TURN_WORDS[item.kind]}</div>;
    case "error":
      return <div className={styles.errorRow}>{clip(item.message, MAX_ROW_TEXT)}</div>;
    case "system":
      return <div className={styles.system}>{clip(item.text, MAX_ROW_TEXT)}</div>;
  }
}
