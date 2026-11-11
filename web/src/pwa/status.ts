import { useSyncExternalStore } from "react";
import { CACHED_AT, FROM_CACHE, classify } from "./offline";

/** What the page knows about where its data comes from. */
export interface OfflineStatus {
  /** The device says it has no network. */
  offline: boolean;
  /** Unix milliseconds of the oldest answer shown from the device's copy, or null when everything shown is live. */
  cachedAt: number | null;
  /** The network is back but some of what is shown is still the device's copy: reading it again is what makes it current. */
  stale: boolean;
}

/** What is shown from the device's copy now, by address: a live answer for the same address makes it current again. */
const fromCopy = new Map<string, number>();
let online = typeof navigator === "undefined" || navigator.onLine !== false;
let status: OfflineStatus = compute();
const listeners = new Set<() => void>();

function compute(): OfflineStatus {
  const oldest = fromCopy.size === 0 ? null : Math.min(...fromCopy.values());
  return { offline: !online, cachedAt: oldest, stale: online && oldest !== null };
}

function publish() {
  const next = compute();
  if (next.offline === status.offline && next.cachedAt === status.cachedAt && next.stale === status.stale) return;
  status = next;
  listeners.forEach((listener) => listener());
}

if (typeof window !== "undefined") {
  window.addEventListener("offline", () => {
    online = false;
    publish();
  });
  // Back online, what is on screen is still the copy until it is read again: the label stays and offers to do that.
  window.addEventListener("online", () => {
    online = true;
    publish();
  });
}

/** Called with every answer of the API: one the worker took from the device's copy is said, and a live one for the same address lifts it. */
export function noteAnswer(path: string, response: Response) {
  if (typeof window === "undefined") return;
  const route = classify({ pathname: `/api${path.split("?")[0]}`, origin: window.location.origin }, "GET", "cors", window.location.origin);
  if (route.kind !== "data") return;
  const cachedAt = Number(response.headers.get(CACHED_AT));
  if (response.headers.get(FROM_CACHE) === "1" && Number.isFinite(cachedAt)) fromCopy.set(path, cachedAt);
  else if (response.ok) fromCopy.delete(path);
  publish();
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
export function resetOfflineStatus(next: { offline?: boolean; copies?: Record<string, number> } = {}) {
  online = !(next.offline ?? false);
  fromCopy.clear();
  Object.entries(next.copies ?? {}).forEach(([path, at]) => fromCopy.set(path, at));
  status = compute();
  listeners.forEach((listener) => listener());
}
