import { beforeEach, describe, expect, it } from "vitest";
import {
  ASSET_CACHE,
  CACHED_AT,
  FROM_CACHE,
  MAX_AGE_MS,
  MAX_BODY_BYTES,
  MAX_ENTRIES,
  SHELL_URL,
  classify,
  clearData,
  createOfflineHandler,
  dataCacheName,
  isExpired,
  sweepExpired,
  type CacheLike,
  type CacheStorageLike,
  type Identity,
  type IdentityStore,
} from "./offline";

const ORIGIN = "http://app.test";
const COMPANY = "0a1b2c3d-0000-4000-8000-000000000001";
const OTHER_COMPANY = "0a1b2c3d-0000-4000-8000-000000000002";
const api = (path: string) => `${ORIGIN}/api/companies/${COMPANY}/${path}`;

/** A `CacheStorage` in memory, keeping the order things were put in. */
class FakeCache implements CacheLike {
  entries = new Map<string, Response>();
  async match(url: string) {
    return this.entries.get(url)?.clone();
  }
  async put(url: string, response: Response) {
    this.entries.delete(url);
    this.entries.set(url, response);
  }
  async delete(url: string) {
    return this.entries.delete(url);
  }
  async keys() {
    return [...this.entries.keys()].map((url) => ({ url }));
  }
}

class FakeCaches implements CacheStorageLike {
  stores = new Map<string, FakeCache>();
  async open(name: string) {
    if (!this.stores.has(name)) this.stores.set(name, new FakeCache());
    return this.stores.get(name)!;
  }
  async keys() {
    return [...this.stores.keys()];
  }
  async delete(name: string) {
    return this.stores.delete(name);
  }
}

class FakeIdentity implements IdentityStore {
  value: Identity | null = null;
  async get() {
    return this.value;
  }
  async set(identity: Identity | null) {
    this.value = identity;
  }
}

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

let caches: FakeCaches;
let identity: FakeIdentity;
let clock: number;
let network: { online: boolean; answers: Map<string, () => Response>; asked: string[] };

function build() {
  return createOfflineHandler({
    caches,
    identity,
    now: () => clock,
    fetch: async (url) => {
      network.asked.push(url);
      if (!network.online) throw new TypeError("Failed to fetch");
      return network.answers.get(url)?.() ?? new Response("not found", { status: 404 });
    },
  });
}

const route = (path: string, method = "GET", mode = "cors") => classify(new URL(path, ORIGIN), method, mode, ORIGIN);

beforeEach(() => {
  caches = new FakeCaches();
  identity = new FakeIdentity();
  identity.value = { userId: "u1", enabled: true };
  clock = 1_800_000_000_000;
  network = { online: true, answers: new Map(), asked: [] };
});

describe("what the worker may keep: an allowlist, not a blocklist", () => {
  it("lets the dashboard's sources and the lists a person opens be cached, query and all", () => {
    for (const path of ["parties", "products?limit=50&q=caf", "entries?sort=-competence_date", "sales", "quotes", "balances?on=2027-01-01", "cash-flow?from=2027-01-01", "stock/positions", "stock/positions/summary", "reports/overdue?on=2027-01-01", "sales-reports?by=product", "statement-lines"]) {
      expect(route(`/api/companies/${COMPANY}/${path}`), path).toEqual({ kind: "data", companyId: COMPANY });
    }
  });

  it("never lets anything else of a company in, whatever the method", () => {
    for (const path of [
      "members",
      "api-keys",
      "webhook-subscriptions",
      "webhook-subscriptions/x/deliveries",
      "connectors",
      "gateway-settings",
      "accounting/exports/entries",
      "accounting/period-lock",
      "fiscal/documents",
      "fiscal/received-invoices",
      "entries/abc/attachments",
      "entries/abc/attachments/def",
      "entries/abc/history",
      "entries/abc/charges/def/link",
      "parties/abc/history",
      "bank-connections",
      "bank-connections/accounts",
      "invitations",
      "collection/settings",
      "stock-reports/valuation",
      "purchase-orders",
      "stock/movements",
      "dre",
    ]) {
      expect(route(`/api/companies/${COMPANY}/${path}`), path).toEqual({ kind: "none" });
    }
    for (const method of ["POST", "PUT", "PATCH", "DELETE"]) expect(route(`/api/companies/${COMPANY}/parties`, method)).toEqual({ kind: "none" });
  });

  it("never touches the session, the public pages, the version of the API for programs, or another origin", () => {
    for (const path of ["/api/auth/login", "/api/auth/logout", "/api/auth/register", "/api/auth/email-confirmation", "/api/auth/password-reset", "/api/public/charges/tok", "/api/public/quotes/tok", "/api/v1/products", "/api/invitations/tok", "/api/companies"]) {
      expect(route(path), path).toEqual({ kind: "none" });
    }
    expect(classify(new URL("https://elsewhere.test/api/companies/" + COMPANY + "/parties"), "GET", "cors", ORIGIN)).toEqual({ kind: "none" });
    // A company id that is not one is not a company.
    expect(route("/api/companies/not-an-id/parties")).toEqual({ kind: "none" });
  });

  it("keeps only who is signed in (the app cannot start offline without it), under its own name", () => {
    expect(route("/api/auth/me")).toEqual({ kind: "data", companyId: "session" });
    expect(route("/api/auth/me", "POST")).toEqual({ kind: "none" });
    expect(route("/api/auth/me/other")).toEqual({ kind: "none" });
  });

  it("keeps the application's own files apart from data, and a page is the shell", () => {
    expect(route("/assets/index-3f9a.js")).toEqual({ kind: "asset" });
    expect(route("/companies/x/parties", "GET", "navigate")).toEqual({ kind: "navigation" });
    expect(route("/random.json", "GET", "cors")).toEqual({ kind: "none" });
  });
});

describe("data, per person and per company", () => {
  it("keeps a good answer under the person and the company, marked with when it was cached, and serves it when the network is gone", async () => {
    network.answers.set(api("parties"), () => json({ items: [{ name: "Maria" }] }));
    const handler = build();
    const live = await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    expect(live.headers.get(FROM_CACHE)).toBeNull();
    expect(await live.json()).toEqual({ items: [{ name: "Maria" }] });
    expect([...caches.stores.keys()]).toEqual([dataCacheName("u1", COMPANY)]);

    network.online = false;
    clock += 60_000;
    const offline = await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    expect(offline.headers.get(FROM_CACHE)).toBe("1");
    expect(offline.headers.get(CACHED_AT)).toBe(String(1_800_000_000_000));
    expect(await offline.json()).toEqual({ items: [{ name: "Maria" }] });
  });

  it("goes to the network first: when it answers, that answer is the one, and a server error is never cached", async () => {
    network.answers.set(api("parties"), () => json({ v: 1 }));
    const handler = build();
    await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    network.answers.set(api("parties"), () => json({ v: 2 }));
    expect(await (await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).json()).toEqual({ v: 2 });
    network.answers.set(api("products"), () => json({ error: "internal error" }, 500));
    const failed = await handler.handle(route(`/api/companies/${COMPANY}/products`), api("products"));
    expect(failed.status).toBe(500);
    const cache = await caches.open(dataCacheName("u1", COMPANY));
    expect(await cache.match(api("products"))).toBeUndefined();
  });

  it("keeps two companies of one person apart, and answers nothing it did not cache", async () => {
    const other = `${ORIGIN}/api/companies/${OTHER_COMPANY}/parties`;
    network.answers.set(api("parties"), () => json({ from: "first" }));
    network.answers.set(other, () => json({ from: "second" }));
    const handler = build();
    await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    await handler.handle(route(`/api/companies/${OTHER_COMPANY}/parties`), other);
    expect([...caches.stores.keys()].sort()).toEqual([dataCacheName("u1", COMPANY), dataCacheName("u1", OTHER_COMPANY)].sort());
    network.online = false;
    await expect(handler.handle(route(`/api/companies/${COMPANY}/products`), api("products"))).rejects.toThrow(TypeError);
    expect(await (await handler.handle(route(`/api/companies/${OTHER_COMPANY}/parties`), other)).json()).toEqual({ from: "second" });
  });

  it("empties what another person cached before anything is read, and never serves it", async () => {
    network.answers.set(api("parties"), () => json({ owner: "u1" }));
    await build().handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    identity.value = { userId: "u2", enabled: true };
    network.online = false;
    await expect(build().handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).rejects.toThrow(TypeError);
    expect([...caches.stores.keys()].filter((name) => name.startsWith("data:u1:"))).toEqual([]);
  });

  it("does nothing at all while the offline mode is off or nobody is known", async () => {
    network.answers.set(api("parties"), () => json({ v: 1 }));
    for (const value of [null, { userId: "u1", enabled: false }]) {
      identity.value = value;
      await build().handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
      expect(caches.stores.size).toBe(0);
    }
    network.online = false;
    await expect(build().handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).rejects.toThrow(TypeError);
  });

  it("clears the data first on a 401 and forgets who was signed in", async () => {
    network.answers.set(api("parties"), () => json({ v: 1 }));
    const handler = build();
    await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    await (await caches.open(ASSET_CACHE)).put("/assets/a.js", new Response("code"));
    network.answers.set(api("products"), () => json({ error: "unauthenticated" }, 401));
    const lost = await handler.handle(route(`/api/companies/${COMPANY}/products`), api("products"));
    expect(lost.status).toBe(401);
    expect([...caches.stores.keys()]).toEqual([ASSET_CACHE]);
    expect(identity.value).toBeNull();
  });

  it("serves nothing older than seven days, and drops it", async () => {
    network.answers.set(api("parties"), () => json({ v: 1 }));
    const handler = build();
    await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    network.online = false;
    clock += MAX_AGE_MS - 1;
    expect((await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).headers.get(FROM_CACHE)).toBe("1");
    clock += 2;
    await expect(handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).rejects.toThrow(TypeError);
    expect(await (await caches.open(dataCacheName("u1", COMPANY))).match(api("parties"))).toBeUndefined();
    expect(isExpired(NaN, clock)).toBe(true);
    expect(isExpired(clock + 3_600_000, clock)).toBe(true);
  });

  it("is bounded: a body over a megabyte is not kept, and only the newest entries stay", async () => {
    const handler = build();
    network.answers.set(api("sales"), () => json({ big: "x".repeat(MAX_BODY_BYTES) }));
    await handler.handle(route(`/api/companies/${COMPANY}/sales`), api("sales"));
    const cache = await caches.open(dataCacheName("u1", COMPANY));
    expect(await cache.keys()).toHaveLength(0);
    for (let i = 0; i < MAX_ENTRIES + 5; i++) {
      network.answers.set(api(`parties?q=${i}`), () => json({ i }));
      await handler.handle(route(`/api/companies/${COMPANY}/parties?q=${i}`), api(`parties?q=${i}`));
    }
    const kept = (await cache.keys()).map((key) => key.url);
    expect(kept).toHaveLength(MAX_ENTRIES);
    expect(kept).not.toContain(api("parties?q=0"));
    expect(kept).toContain(api(`parties?q=${MAX_ENTRIES + 4}`));
  });

  it("does not keep what is not JSON", async () => {
    network.answers.set(api("parties"), () => new Response("<html>login</html>", { status: 200, headers: { "content-type": "text/html" } }));
    await build().handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    expect(caches.stores.size).toBe(0);
  });
});

describe("the application's own files", () => {
  it("serves assets from the cache once seen, and the shell when the network is gone, never any data", async () => {
    network.answers.set(`${ORIGIN}/assets/app-1.js`, () => new Response("code", { status: 200 }));
    network.answers.set(SHELL_URL, () => new Response("<div id=root>", { status: 200 }));
    const handler = build();
    await handler.handle(route("/assets/app-1.js"), `${ORIGIN}/assets/app-1.js`);
    await handler.handle(route("/anything", "GET", "navigate"), `${ORIGIN}/anything`);
    network.online = false;
    expect(await (await handler.handle(route("/assets/app-1.js"), `${ORIGIN}/assets/app-1.js`)).text()).toBe("code");
    expect(await (await handler.handle(route("/other/page", "GET", "navigate"), `${ORIGIN}/other/page`)).text()).toBe("<div id=root>");
    expect([...caches.stores.keys()]).toEqual([ASSET_CACHE]);
  });

  it("clearing data leaves them alone", async () => {
    await (await caches.open(ASSET_CACHE)).put("/assets/a.js", new Response("code"));
    await (await caches.open(dataCacheName("u1", COMPANY))).put("x", json({}));
    await clearData(caches);
    expect([...caches.stores.keys()]).toEqual([ASSET_CACHE]);
  });
});

describe("what goes wrong around the cache must not reach the person", () => {
  it("a clearing that comes while a request is away is not undone by its answer", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const handler = createOfflineHandler({
      caches,
      identity,
      now: () => clock,
      fetch: async () => {
        await gate;
        return json({ owner: "u1" });
      },
    });
    const pending = handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    // The person signs out (the page clears the caches and the identity) while the answer is on its way.
    await clearData(caches);
    await identity.set(null);
    release();
    expect((await pending).status).toBe(200);
    expect(caches.stores.size).toBe(0);
  });

  it("a different person who signed in meanwhile is not given what the first person asked for", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const handler = createOfflineHandler({ caches, identity, now: () => clock, fetch: async () => (await gate, json({ owner: "u1" })) });
    const pending = handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    await identity.set({ userId: "u2", enabled: true });
    release();
    await pending;
    expect([...caches.stores.keys()].filter((name) => name.startsWith("data:u1:"))).toEqual([]);
  });

  it("the profile is kept only if it is the profile of the person the page named", async () => {
    network.answers.set(`${ORIGIN}/api/auth/me`, () => json({ user: { id: "u2" } }));
    await build().handle(route("/api/auth/me"), `${ORIGIN}/api/auth/me`);
    expect(caches.stores.size).toBe(0);
    network.answers.set(`${ORIGIN}/api/auth/me`, () => json({ user: { id: "u1" } }));
    await build().handle(route("/api/auth/me"), `${ORIGIN}/api/auth/me`);
    expect([...caches.stores.keys()]).toEqual([dataCacheName("u1", "session")]);
  });

  it("a storage that fails never keeps the network's answer from the page, off or on", async () => {
    network.answers.set(api("parties"), () => json({ v: 1 }));
    const broken = (): CacheStorageLike => ({ open: async () => Promise.reject(new Error("quota")), keys: async () => Promise.reject(new Error("denied")), delete: async () => false });
    const handler = createOfflineHandler({ caches: broken(), identity, now: () => clock, fetch: async (url) => (network.asked.push(url), json({ v: 1 })) });
    expect(await (await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).json()).toEqual({ v: 1 });
    const noIdentity: IdentityStore = { get: async () => Promise.reject(new Error("blocked")), set: async () => undefined };
    const blocked = createOfflineHandler({ caches, identity: noIdentity, now: () => clock, fetch: async () => json({ v: 2 }) });
    expect(await (await blocked.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).json()).toEqual({ v: 2 });
    // And a lost session reaches the page even when the clearing fails.
    const lost = createOfflineHandler({ caches: broken(), identity, now: () => clock, fetch: async () => json({}, 401) });
    expect((await lost.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).status).toBe(401);
  });

  it("a server that is there but cannot answer (502, 503, 504) is answered from the copy, and a 500 of the application is not", async () => {
    const handler = build();
    network.answers.set(api("parties"), () => json({ v: 1 }));
    await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"));
    for (const status of [502, 503, 504]) {
      network.answers.set(api("parties"), () => new Response("bad gateway", { status }));
      expect((await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).headers.get(FROM_CACHE)).toBe("1");
    }
    network.answers.set(api("parties"), () => json({ error: "internal error" }, 500));
    expect((await handler.handle(route(`/api/companies/${COMPANY}/parties`), api("parties"))).status).toBe(500);
    // With no copy, the 502 is what the page gets.
    network.answers.set(api("products"), () => new Response("bad gateway", { status: 502 }));
    expect((await handler.handle(route(`/api/companies/${COMPANY}/products`), api("products"))).status).toBe(502);
  });

  it("entries older than seven days are swept even if nobody asks for them again", async () => {
    const handler = build();
    network.answers.set(api("parties?q=maria"), () => json({ v: 1 }));
    await handler.handle(route(`/api/companies/${COMPANY}/parties?q=maria`), api("parties?q=maria"));
    clock += MAX_AGE_MS + 1;
    await sweepExpired(caches, clock);
    expect(caches.stores.size).toBe(0);
    // The worker also sweeps on its own, once in a while, when it keeps something new.
    network.answers.set(api("parties?q=ana"), () => json({ v: 1 }));
    await handler.handle(route(`/api/companies/${COMPANY}/parties?q=ana`), api("parties?q=ana"));
    clock += MAX_AGE_MS + 2 * 3_600_000;
    network.answers.set(api("products"), () => json({ v: 2 }));
    await handler.handle(route(`/api/companies/${COMPANY}/products`), api("products"));
    const names = await (await caches.open(dataCacheName("u1", COMPANY))).keys();
    expect(names.map((key) => key.url)).toEqual([api("products")]);
  });

  it("what is not a hashed file is not kept for good, a missing file answered with the page is not kept, and the worker's own address is the network's", async () => {
    expect(route("/favicon.svg")).toEqual({ kind: "none" });
    expect(route("/site.webmanifest")).toEqual({ kind: "none" });
    expect(route("/sw.js", "GET", "navigate")).toEqual({ kind: "none" });
    network.answers.set(`${ORIGIN}/assets/gone.js`, () => new Response("<html>", { status: 200, headers: { "content-type": "text/html" } }));
    await build().handle(route("/assets/gone.js"), `${ORIGIN}/assets/gone.js`);
    expect(await (await caches.open(ASSET_CACHE)).match(`${ORIGIN}/assets/gone.js`)).toBeUndefined();
  });

  it("paths that are not what they look like are the network's", () => {
    for (const path of [`/api/companies/${COMPANY}/entries/`, `/api/companies/${COMPANY}/PARTIES`, `/api//companies/${COMPANY}/parties`, `//api/companies/${COMPANY}/parties`, `/api/companies/${COMPANY}/parties%2fx`, `/api/companies/${COMPANY}/parties/../members`]) {
      const result = route(path);
      // A trailing slash is the same list; anything else that is not exactly an allowlisted path is the network's.
      if (path.endsWith("/entries/")) expect(result).toEqual({ kind: "data", companyId: COMPANY });
      else expect(result.kind, path).not.toBe("data");
    }
  });
});
