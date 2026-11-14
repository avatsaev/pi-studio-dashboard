import type { AgentStatus, StatusChip } from "../lib/types";
import { Button } from "./Button";
import { FilterChip } from "./FilterChip";
import styles from "./StatusFilterChips.module.css";

/**
 * The status filter row: one toggle chip per bucket the shell sent, with its count (computed over
 * the daemon-filtered cards). A click sends the new set of selected statuses; whether a chip is
 * active is the view's answer, never local state. "Clear filters" appears while any filter is on.
 */
export function StatusFilterChips({
  chips,
  filtered,
  onChange,
  onClear,
}: {
  chips: StatusChip[];
  /** Any filter at all is on (a daemon chip counts). */
  filtered: boolean;
  onChange: (statuses: AgentStatus[]) => void;
  onClear: () => void;
}) {
  if (chips.length === 0 && !filtered) {
    return null;
  }
  const toggle = (bucket: AgentStatus) => {
    const selected = chips.filter((chip) => chip.selected).map((chip) => chip.bucket);
    onChange(selected.includes(bucket) ? selected.filter((b) => b !== bucket) : [...selected, bucket]);
  };
  return (
    <ul className={styles.chips} aria-label="Status filters">
      {chips.map((chip) => (
        <li key={chip.bucket}>
          <FilterChip label={chip.bucket} count={chip.count} status={chip.bucket} active={chip.selected} title={`Show ${chip.bucket} agents only`} onToggle={() => toggle(chip.bucket)} />
        </li>
      ))}
      {filtered && (
        <li>
          <Button onClick={onClear}>Clear filters</Button>
        </li>
      )}
    </ul>
  );
}
