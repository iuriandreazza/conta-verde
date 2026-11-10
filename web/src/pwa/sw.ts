// The service worker (ADR 0091): it keeps the application's own files so the app opens offline, and, only for who turned the offline
// mode on, the allowlisted reads of the company. All the rules are in `offline.ts`; this only wires them to the worker's events.
import { indexedDbIdentity } from "./identityStore";
import { ASSET_CACHE, classify, createOfflineHandler, type CacheStorageLike } from "./offline";

// The DOM types are the ones this project compiles with; the worker's global has the few members used here.
interface WorkerScope {
  location: { origin: string };
  skipWaiting(): Promise<void>;
  clients: { claim(): Promise<void> };
  addEventListener(type: string, listener: (event: never) => void): void;
}
interface Waitable {
  waitUntil(promise: Promise<unknown>): void;
}
interface FetchEventLike {
  request: Request;
  respondWith(response: Promise<Response>): void;
}

const scope = self as unknown as WorkerScope;
const storage = caches as unknown as CacheStorageLike;
const handler = createOfflineHandler({ caches: storage, fetch: (url) => fetch(url), identity: indexedDbIdentity(), now: () => Date.now() });

scope.addEventListener("install", ((event: Waitable) => event.waitUntil(scope.skipWaiting())) as (event: never) => void);

scope.addEventListener("activate", ((event: Waitable) => {
  event.waitUntil(
    (async () => {
      // Only the caches of this worker's own versions stay: a new build's assets are fetched by their new names.
      for (const name of await storage.keys()) if (!name.startsWith("data:") && name !== ASSET_CACHE) await storage.delete(name);
      await scope.clients.claim();
    })(),
  );
}) as (event: never) => void);

scope.addEventListener("fetch", ((event: FetchEventLike) => {
  const { request } = event;
  const route = classify(new URL(request.url), request.method, request.mode, scope.location.origin);
  // Not the worker's business: the browser goes to the network as if there were no worker.
  if (route.kind === "none") return;
  event.respondWith(handler.handle(route, request.url));
}) as (event: never) => void);
