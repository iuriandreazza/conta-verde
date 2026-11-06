import { useEffect, useState } from "react";
import { errorMessage } from "../api";

export interface Loaded<T> {
  data: T | null;
  error: string | null;
}

/**
 * What one card of the dashboard shows: its own request, its own failure. Every card asks for what it
 * needs and none waits for another, so one that fails says so by itself and the rest still draw.
 * `deps` are what the request depends on; a reply to an earlier one is dropped.
 */
export function useLoaded<T>(load: () => Promise<T>, deps: unknown[]): Loaded<T> {
  const [state, setState] = useState<Loaded<T>>({ data: null, error: null });
  useEffect(() => {
    let current = true;
    // What was on screen belongs to the earlier question.
    setState({ data: null, error: null });
    load()
      .then((data) => current && setState({ data, error: null }))
      .catch((failure: unknown) => current && setState({ data: null, error: errorMessage(failure) }));
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return state;
}
