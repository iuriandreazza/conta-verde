import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { errorMessage } from "../api";
import { useDebounced } from "./useDebounced";

export interface Page<T> {
  items: T[];
  next_cursor: string | null;
}

const SEARCH_DELAY_MS = 300;

/**
 * A list with keyset pagination whose search text, filters and order change while it is on screen
 * (ADR 0018). The search text, when the list has one, follows the box once typing pauses; every other filter applies at
 * once. `fetchPage` is called with the applied filters and, for the next pages, the cursor that
 * the same filters produced.
 *
 * Layout remounts the page whenever the path changes, so what `fetchPage` closes over (the company)
 * never changes within one instance.
 */
export function usePagedList<T, F extends object>(
  fetchPage: (filters: F, after?: string) => Promise<Page<T>>,
  filters: F,
) {
  const [items, setItems] = useState<T[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Filters are plain JSON, so their serialization is a stable identity: `applied` only changes
  // when a value does, not on every keystroke in the search box.
  const typed = (filters as { q?: unknown }).q;
  const searchText = useDebounced(typeof typed === "string" ? typed.trim() : "", SEARCH_DELAY_MS);
  const appliedKey = JSON.stringify("q" in filters ? { ...filters, q: searchText } : filters);
  const applied = useMemo(() => JSON.parse(appliedKey) as F, [appliedKey]);

  const fetchRef = useRef(fetchPage);
  fetchRef.current = fetchPage;
  const appliedRef = useRef(applied);
  appliedRef.current = applied;
  // Counts the requests for a first page: a reply is only used while it is still the latest, so a
  // slow answer to an earlier search can neither replace a newer one nor receive its "load more".
  const latestListing = useRef(0);

  // Only the very first load shows the spinner; refreshing after an insert or a new filter keeps
  // the rows on screen until the new ones arrive.
  const load = useCallback(async (filtersToApply: F) => {
    const listing = ++latestListing.current;
    setError(null);
    // The old cursor belongs to the old filters: "load more" must not be clickable until the new
    // first page says whether there is a next one.
    setNextCursor(null);
    try {
      const page = await fetchRef.current(filtersToApply);
      if (listing === latestListing.current) {
        setItems(page.items);
        setNextCursor(page.next_cursor);
      }
    } catch (failure) {
      if (listing === latestListing.current) {
        // Rows of the previous filters under controls that show the new ones would read as its result.
        setItems([]);
        setError(errorMessage(failure));
      }
    } finally {
      if (listing === latestListing.current) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    void load(applied);
  }, [load, applied]);

  /** Loads the first page again with the filters on screen at the moment it is called. */
  const reload = useCallback(() => load(appliedRef.current), [load]);

  async function loadMore() {
    if (nextCursor === null || loadingMore) {
      return;
    }
    const listing = latestListing.current;
    setLoadingMore(true);
    setError(null);
    try {
      const page = await fetchRef.current(applied, nextCursor);
      if (listing === latestListing.current) {
        setItems((current) => [...current, ...page.items]);
        setNextCursor(page.next_cursor);
      }
    } catch (failure) {
      if (listing === latestListing.current) {
        setError(errorMessage(failure));
      }
    } finally {
      setLoadingMore(false);
    }
  }

  return { items, setItems, nextCursor, loading, loadingMore, error, setError, applied, reload, loadMore };
}
