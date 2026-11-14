import { useEffect, useState } from "react";

/**
 * The local clock as of the last time `view` changed (and once a minute besides), for "x ago" and
 * "stale since" texts. Reading the clock while rendering would make a render impure, so it is state.
 */
export function useNow(view: unknown): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, []);
  // The clock is read when a view arrives: a card's age is shown against the moment its data was.
  // oxlint-disable-next-line react/exhaustive-deps
  useEffect(() => setNow(Date.now()), [view]);
  return now;
}
