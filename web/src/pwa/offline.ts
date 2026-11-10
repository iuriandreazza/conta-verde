// The rules of the offline mode (ADR 0091), written as a module with no globals so that the worker, the page and the tests all run
// the same code: the worker passes the real `caches`, `fetch` and IndexedDB, a test passes fakes. A cache is another place a
// company's data lives, so what may enter it is an allowlist and every other request goes to the network untouched.

export const DATA_PREFIX = "data:";
export const ASSET_CACHE = "assets-v1";
export const SHELL_URL = "/index.html";
/** The most a cached answer may weigh, in bytes. */
export const MAX_BODY_BYTES = 1_000_000;
/** The most files of the application kept, so that the builds of old versions do not pile up. */
export const MAX_ASSETS = 120;
/** The most answers kept for one person in one company: the oldest goes first. */
export const MAX_ENTRIES = 40;
/** How long a cached answer may be served, in milliseconds, even when nothing cleared it (a session that ended while the device was off). */
export const MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;
/** When an answer was cached (Unix milliseconds), written by the worker. */
export const CACHED_AT = "x-offline-cached-at";
/** Present on an answer that came from the cache and not from the server. */
export const FROM_CACHE = "x-offline-from-cache";

/** What of a company may be read offline: the dashboard's sources and the lists a person opens. Nothing else, ever. */
const DATA_ALLOWLIST = [
  "parties",
  "products",
  "entries",
  "sales",
  "quotes",
  "balances",
  "cash-flow",
  "stock/positions",
  "stock/positions/summary",
  "reports/overdue",
  "sales-reports",
  "statement-lines",
];

/** What `GET /auth/me` is kept under: the application cannot start offline without knowing who is signed in and what they may do. */
export const SESSION_SCOPE = "session";

const COMPANY_PATH = /^\/api\/companies\/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\/(.+)$/i;

export type Route =
  | { kind: "data"; companyId: string }
  /** The application's own files, named by their content: not data. */
  | { kind: "asset" }
  /** A page of the application: the shell without data. */
  | { kind: "navigation" }
  /** Not the worker's business: the network answers. */
  | { kind: "none" };

/** The part of a `URL` the classification reads, so that the page can ask without making one. */
export interface UrlLike {
  pathname: string;
  origin: string;
}

/** What a request is to the worker. Synchronous, because the worker must decide at once whether it answers. */
export function classify(url: UrlLike, method: string, mode: string, origin: string): Route {
  if (method !== "GET" || url.origin !== origin) return { kind: "none" };
  const path = url.pathname.replace(/\/+$/, "");
  const company = COMPANY_PATH.exec(path);
  if (company) {
    return DATA_ALLOWLIST.includes(company[2]) ? { kind: "data", companyId: company[1].toLowerCase() } : { kind: "none" };
  }
  // The one thing of the session that is kept (and only while the offline mode is on): the same rules and the same moments of
  // clearing as the data, under the person's own name. Sign-in, sign-out, registration and every other /auth call are the network's.
  if (path === "/api/auth/me") return { kind: "data", companyId: SESSION_SCOPE };
  if (path.startsWith("/api")) return { kind: "none" };
  // Only what is named by its content is kept for good: an unhashed file (the icons, the manifest) would never be updated.
  if (path.startsWith("/assets/")) return { kind: "asset" };
  // The worker's own address is never answered with the page.
  if (path === "/sw.js") return { kind: "none" };
  return mode === "navigate" ? { kind: "navigation" } : { kind: "none" };
}

export const dataCacheName = (userId: string, companyId: string) => `${DATA_PREFIX}${userId}:${companyId}`;

export const isExpired = (cachedAt: number, now: number) => !Number.isFinite(cachedAt) || now - cachedAt > MAX_AGE_MS || cachedAt > now + 60_000;

/** Who is signed in on this device, told by the page (the worker cannot read the HttpOnly session cookie), and whether they turned the offline mode on. */
export interface Identity {
  userId: string;
  enabled: boolean;
}

export interface IdentityStore {
  get(): Promise<Identity | null>;
  set(identity: Identity | null): Promise<void>;
}

/** The part of `CacheStorage` and `Cache` the rules use, so that a test can fake them. */
export interface CacheLike {
  match(request: string): Promise<Response | undefined>;
  put(request: string, response: Response): Promise<void>;
  delete(request: string): Promise<boolean>;
  keys(): Promise<readonly { url: string }[]>;
}

export interface CacheStorageLike {
  open(name: string): Promise<CacheLike>;
  keys(): Promise<string[]>;
  delete(name: string): Promise<boolean>;
}

/** Empties every data cache: the application's own files are not data and stay. */
export async function clearData(caches: CacheStorageLike): Promise<void> {
  for (const name of await caches.keys()) {
    if (name.startsWith(DATA_PREFIX)) await caches.delete(name);
  }
}

/** Empties the data caches of every person but `userId`: a different user never reads, or leaves, what another cached. */
export async function clearOthers(caches: CacheStorageLike, userId: string): Promise<void> {
  const mine = `${DATA_PREFIX}${userId}:`;
  for (const name of await caches.keys()) {
    if (name.startsWith(DATA_PREFIX) && !name.startsWith(mine)) await caches.delete(name);
  }
}

interface Dependencies {
  caches: CacheStorageLike;
  fetch: (url: string) => Promise<Response>;
  identity: IdentityStore;
  now: () => number;
}

const withHeaders = (response: Response, body: ArrayBuffer, extra: Record<string, string>) => {
  const headers = new Headers(response.headers);
  for (const [name, value] of Object.entries(extra)) headers.set(name, value);
  return new Response(body, { status: response.status, statusText: response.statusText, headers });
};

/** Deletes what is older than the time limit, in every data cache, and the caches that end up empty: an entry nobody asks for again must not outlive it. */
export async function sweepExpired(caches: CacheStorageLike, now: number): Promise<void> {
  for (const name of await caches.keys()) {
    if (!name.startsWith(DATA_PREFIX)) continue;
    const cache = await caches.open(name);
    let left = 0;
    for (const key of await cache.keys()) {
      const held = await cache.match(key.url);
      if (held && !isExpired(Number(held.headers.get(CACHED_AT)), now)) left++;
      else await cache.delete(key.url);
    }
    if (left === 0) await caches.delete(name);
  }
}

/** How often a worker looks for expired entries, at most. */
const SWEEP_EVERY_MS = 60 * 60 * 1000;
/** The answers of a server that is there but cannot answer: the copy is served in their place. */
const UNREACHABLE = new Set([502, 503, 504]);

/** What the worker does with a request the classification gave it. */
export function createOfflineHandler({ caches, fetch, identity, now }: Dependencies) {
  let lastSweep = 0;

  /** The copy of an answer, if there is one that has not expired. A storage that fails is a copy that is not there. */
  async function copy(name: string, url: string): Promise<Response | null> {
    try {
      const cache = await caches.open(name);
      const held = await cache.match(url);
      if (!held) return null;
      if (isExpired(Number(held.headers.get(CACHED_AT)), now())) {
        await cache.delete(url);
        return null;
      }
      return withHeaders(held, await held.arrayBuffer(), { [FROM_CACHE]: "1" });
    } catch {
      return null;
    }
  }

  /** Whether the person this worker was told about is still the one, and still wants a copy: a clearing may have come while the request was away. */
  async function stillMine(userId: string): Promise<boolean> {
    const who = await identity.get();
    return who !== null && who.enabled && who.userId === userId;
  }

  async function keep(who: Identity, name: string, companyId: string, url: string, response: Response): Promise<void> {
    if (response.status !== 200 || !(response.headers.get("content-type") ?? "").includes("json")) return;
    const body = await response.clone().arrayBuffer();
    if (body.byteLength > MAX_BODY_BYTES) return;
    // The profile is kept only if it is the profile of the person the page named: another tab may have signed someone else in.
    if (companyId === SESSION_SCOPE) {
      const profile = JSON.parse(new TextDecoder().decode(body)) as { user?: { id?: string } };
      if (profile.user?.id !== who.userId) return;
    }
    // Nothing is written for someone who signed out, or turned the mode off, while the request was on its way.
    if (!(await stillMine(who.userId))) return;
    const cache = await caches.open(name);
    await cache.put(url, withHeaders(response, body, { [CACHED_AT]: String(now()) }));
    const entries = await cache.keys();
    for (const old of entries.slice(0, Math.max(0, entries.length - MAX_ENTRIES))) await cache.delete(old.url);
    if (now() - lastSweep > SWEEP_EVERY_MS) {
      lastSweep = now();
      await sweepExpired(caches, now());
    }
  }

  /**
   * Network first, the copy when it cannot answer. **Whatever goes wrong with the storage, the network's answer still reaches the
   * page**: the offline mode is a convenience and never what stands between a person and the server.
   */
  async function data(url: string, companyId: string): Promise<Response> {
    let who: Identity | null;
    try {
      who = await identity.get();
    } catch {
      return fetch(url);
    }
    // Off, or nobody known: nothing is read from or written to a cache.
    if (!who || !who.enabled) return fetch(url);
    try {
      await clearOthers(caches, who.userId);
    } catch {
      return fetch(url);
    }
    const name = dataCacheName(who.userId, companyId);
    let response: Response;
    try {
      response = await fetch(url);
    } catch (error) {
      const held = await copy(name, url);
      if (held) return held;
      throw error;
    }
    // A lost session is a cleared device: the data of whoever was signed in does not outlive it.
    if (response.status === 401) {
      try {
        await clearData(caches);
        await identity.set(null);
      } catch {
        // The 401 still reaches the page, which clears what it can reach itself.
      }
      return response;
    }
    if (UNREACHABLE.has(response.status)) return (await copy(name, url)) ?? response;
    try {
      await keep(who, name, companyId, url, response);
    } catch {
      // A full or refused storage keeps nothing: the answer is the same.
    }
    return response;
  }

  /** The application's own files, named by their content (`/assets/…`): not data. A missing file that the server answers with the page is not kept. */
  async function asset(url: string): Promise<Response> {
    const cache = await caches.open(ASSET_CACHE);
    const held = await cache.match(url);
    if (held) return held;
    const response = await fetch(url);
    if (response.status === 200 && !(response.headers.get("content-type") ?? "").includes("html")) {
      await cache.put(url, response.clone());
      const entries = await cache.keys();
      for (const old of entries.filter((entry) => entry.url !== SHELL_URL).slice(0, Math.max(0, entries.length - MAX_ASSETS))) await cache.delete(old.url);
    }
    return response;
  }

  async function navigation(): Promise<Response> {
    const cache = await caches.open(ASSET_CACHE);
    try {
      const response = await fetch(SHELL_URL);
      if (response.status === 200) await cache.put(SHELL_URL, response.clone());
      return response;
    } catch (error) {
      const held = await cache.match(SHELL_URL);
      if (held) return held;
      throw error;
    }
  }

  return {
    handle(route: Route, url: string): Promise<Response> {
      switch (route.kind) {
        case "data":
          return data(url, route.companyId);
        case "asset":
          return asset(url);
        case "navigation":
          return navigation();
        case "none":
          return fetch(url);
      }
    },
  };
}
