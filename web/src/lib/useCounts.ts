import { useEffect, useState } from "react";

/**
 * Counts that sit beside a list (the tiles above it): read again when the list's rows change and once the list
 * has loaded, so what they say follows what is on screen. A failure leaves them blank, because the list reports
 * its own; the old numbers stay up while new ones are on their way.
 */
export function useCounts<T>(load: () => Promise<T>, rows: unknown, ready: boolean): T | null {
  const [counts, setCounts] = useState<T | null>(null);
  useEffect(() => {
    if (!ready) return;
    let current = true;
    load()
      .then((loaded) => current && setCounts(loaded))
      .catch(() => current && setCounts(null));
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, ready]);
  return counts;
}
