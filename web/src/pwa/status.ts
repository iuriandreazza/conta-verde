import { useSyncExternalStore } from "react";
import { CACHED_AT, FROM_CACHE, classify } from "./offline";

/** What the page knows about where its data comes from. */
export interface OfflineStatus {
  /** The device says it has no network. */
  offline: boolean;
  /** Unix milliseconds of the oldest answer shown from the device's copy, or null when everything shown is live. */
  cachedAt: number | null;
}

let status: OfflineStatus = { offline: typeof navigator !== "undefined" && navigator.onLine === false, cachedAt: null };
const listeners = new Set<() => void>();

function change(next: OfflineStatus) {
  if (next.offline === status.offline && next.cachedAt === status.cachedAt) return;
  status = next;
  listeners.forEach((listener) => listener());
}

if (typeof window !== "undefined") {
  window.addEventListener("offline", () => change({ ...status, offline: true }));
  // Back online, the next reads are live again: nothing shown stays labeled as old until it is read again.
  window.addEventListener("online", () => change({ offline: false, cachedAt: null }));
}

/** Called with every answer of the API: one the worker took from the device's copy is said, and a live one of the same kind lifts the label. */
export function noteAnswer(path: string, response: Response) {
  if (typeof window === "undefined") return;
  const route = classify({ pathname: `/api${path.split("?")[0]}`, origin: window.location.origin }, "GET", "cors", window.location.origin);
  if (route.kind !== "data") return;
  const from = response.headers.get(FROM_CACHE) === "1" ? Number(response.headers.get(CACHED_AT)) : null;
  if (from !== null && Number.isFinite(from)) change({ ...status, cachedAt: status.cachedAt === null ? from : Math.min(status.cachedAt, from) });
  else if (from === null) change({ ...status, cachedAt: null });
}

export function useOfflineStatus(): OfflineStatus {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => status,
  );
}

/** For tests. */
export function resetOfflineStatus(next: OfflineStatus = { offline: false, cachedAt: null }) {
  status = next;
  listeners.forEach((listener) => listener());
}
