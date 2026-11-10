// The page's side of the offline mode (ADR 0091): it registers the worker, tells it who is signed in, and clears the device's copy of the
// data at sign-out and when the session is lost. Cache Storage and IndexedDB are the page's to reach too, so clearing never depends on
// the worker being awake, and it comes **before** the network is asked (signing out offline still clears).
import { indexedDbIdentity } from "./identityStore";
import { clearData, clearOthers, type CacheStorageLike, type IdentityStore } from "./offline";

/** Per person, not per browser: someone else signing in on this device never inherits a mode the first person turned on. */
const enabledKey = (userId: string) => `conta-verde.offline.${userId}`;

interface Deps {
  caches: CacheStorageLike | null;
  identity: IdentityStore;
}

const nothing: IdentityStore = { get: async () => null, set: async () => undefined };

function realDeps(): Deps {
  return {
    caches: typeof caches === "undefined" ? null : (caches as unknown as CacheStorageLike),
    identity: typeof indexedDB === "undefined" ? nothing : indexedDbIdentity(),
  };
}

/** Whether this person turned the offline mode on, on this device. Storage may be missing or refuse: then it is off. */
export function isOfflineEnabled(userId: string): boolean {
  try {
    return window.localStorage.getItem(enabledKey(userId)) === "on";
  } catch {
    return false;
  }
}

function rememberEnabled(userId: string, on: boolean) {
  try {
    if (on) window.localStorage.setItem(enabledKey(userId), "on");
    else window.localStorage.removeItem(enabledKey(userId));
  } catch {
    // Not remembered: the identity record below still says what the worker does.
  }
}

/** Registers the worker, in a production build only (development serves no worker, and a stale one would hide changes). */
export function registerServiceWorker() {
  if (!import.meta.env.PROD || !("serviceWorker" in navigator)) return;
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("/sw.js", { scope: "/" }).catch((error: unknown) => console.error("service worker not registered", error));
  });
}

/** Empties this device's copy of the company's data and forgets who was signed in. Never throws: a device that cannot clear says nothing more. */
export async function clearOfflineData(deps: Deps = realDeps()): Promise<void> {
  try {
    if (deps.caches) await clearData(deps.caches);
    await deps.identity.set(null);
  } catch (error) {
    console.error("could not clear the offline data", error);
  }
}

/**
 * Tells the worker who is signed in. Someone else's copy is emptied first; with the mode off nothing is kept; signed out, everything
 * goes. Called whenever the session is known or changes.
 */
export async function syncIdentity(userId: string | null, deps: Deps = realDeps()): Promise<void> {
  if (userId === null) return clearOfflineData(deps);
  try {
    const enabled = isOfflineEnabled(userId);
    if (deps.caches) {
      if (enabled) await clearOthers(deps.caches, userId);
      else await clearData(deps.caches);
    }
    await deps.identity.set({ userId, enabled });
  } catch (error) {
    console.error("could not tell the worker who is signed in", error);
  }
}

/** Turns the offline mode on or off on this device. Off empties the copy at once. */
export async function setOfflineEnabled(on: boolean, userId: string, deps: Deps = realDeps()): Promise<void> {
  rememberEnabled(userId, on);
  await syncIdentity(userId, deps);
}
