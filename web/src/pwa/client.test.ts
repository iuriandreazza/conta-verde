import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearOfflineData, isOfflineEnabled, setOfflineEnabled, syncIdentity } from "./client";
import { dataCacheName, type CacheStorageLike, type Identity, type IdentityStore } from "./offline";

const names = new Set<string>();
const fakeCaches: CacheStorageLike = {
  // Every cache holds one fresh answer, so that a sweep for what expired finds nothing to remove.
  async open() {
    const fresh = new Response("{}", { headers: { "x-offline-cached-at": String(Date.now()) } });
    return { match: async () => fresh.clone(), delete: async () => true, keys: async () => [{ url: "x" }], put: async () => undefined };
  },
  async keys() {
    return [...names];
  },
  async delete(name) {
    return names.delete(name);
  },
};

let held: Identity | null;
const identity: IdentityStore = {
  async get() {
    return held;
  },
  async set(value) {
    held = value;
  },
};
const deps = { caches: fakeCaches, identity };

beforeEach(() => {
  names.clear();
  held = null;
  window.localStorage.clear();
});
afterEach(() => vi.restoreAllMocks());

describe("telling the worker who is signed in", () => {
  it("keeps nothing while the offline mode is off, and says so to the worker", async () => {
    names.add(dataCacheName("u1", "c1")).add("assets-v1");
    await syncIdentity("u1", deps);
    expect([...names]).toEqual(["assets-v1"]);
    expect(held).toEqual({ userId: "u1", enabled: false });
  });

  it("turned on, empties what someone else cached and keeps the person's own", async () => {
    window.localStorage.setItem("conta-verde.offline.u1", "on");
    names.add(dataCacheName("u1", "c1")).add(dataCacheName("u2", "c1")).add("assets-v1");
    await syncIdentity("u1", deps);
    expect([...names].sort()).toEqual(["assets-v1", dataCacheName("u1", "c1")].sort());
    expect(held).toEqual({ userId: "u1", enabled: true });
  });

  it("turning it on and off from the page, off clears the copy at once", async () => {
    await setOfflineEnabled(true, "u1", deps);
    expect(isOfflineEnabled("u1")).toBe(true);
    expect(isOfflineEnabled("u2")).toBe(false);
    expect(held).toEqual({ userId: "u1", enabled: true });
    names.add(dataCacheName("u1", "c1"));
    await setOfflineEnabled(false, "u1", deps);
    expect(isOfflineEnabled("u1")).toBe(false);
    expect([...names]).toEqual([]);
    expect(held).toEqual({ userId: "u1", enabled: false });
  });

  it("signed out, everything goes and nobody is known", async () => {
    window.localStorage.setItem("conta-verde.offline.u1", "on");
    held = { userId: "u1", enabled: true };
    names.add(dataCacheName("u1", "c1")).add("assets-v1");
    await syncIdentity(null, deps);
    expect([...names]).toEqual(["assets-v1"]);
    expect(held).toBeNull();
  });

  it("never throws, whatever the device refuses", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const broken = { caches: { ...fakeCaches, keys: async () => Promise.reject(new Error("denied")) }, identity };
    await expect(clearOfflineData(broken)).resolves.toBeUndefined();
    // The caches failing does not keep the worker from forgetting who was signed in.
    held = { userId: "u1", enabled: true };
    await clearOfflineData(broken);
    expect(held).toBeNull();
    await expect(syncIdentity("u1", broken)).resolves.toBeUndefined();
    expect(error).toHaveBeenCalled();
    // A device with no storage at all: the mode is just off.
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(isOfflineEnabled("u1")).toBe(false);
  });
});

describe("a mode one person turned on is theirs", () => {
  it("someone else signing in on the same device starts with it off, and the first person's copy is emptied", async () => {
    await setOfflineEnabled(true, "u1", deps);
    names.add(dataCacheName("u1", "c1"));
    await syncIdentity("u2", deps);
    expect(held).toEqual({ userId: "u2", enabled: false });
    expect([...names]).toEqual([]);
    expect(isOfflineEnabled("u1")).toBe(true);
  });
});

describe("turning it on tells the truth about whether it worked", () => {
  it("says so when the device refuses the worker's record, and leaves the choice as it was", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const refusing = { caches: fakeCaches, identity: { get: async () => null, set: async () => Promise.reject(new Error("blocked")) } };
    await expect(setOfflineEnabled(true, "u1", refusing)).rejects.toThrow("blocked");
    expect(isOfflineEnabled("u1")).toBe(false);
  });

  it("says so when the browser keeps no settings, and puts the worker's record back off", async () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    await expect(setOfflineEnabled(true, "u1", deps)).rejects.toThrow("keeps no settings");
    expect(held).toEqual({ userId: "u1", enabled: false });
  });

  it("empties what expired when the page loads, with the mode on", async () => {
    window.localStorage.setItem("conta-verde.offline.u1", "on");
    const caches = new (class {
      stores = new Map<string, Map<string, Response>>([[dataCacheName("u1", "c1"), new Map([["x", new Response("{}", { headers: { "x-offline-cached-at": "1" } })]])]]);
      async keys() {
        return [...this.stores.keys()];
      }
      async delete(name: string) {
        return this.stores.delete(name);
      }
      async open(name: string) {
        const store = this.stores.get(name)!;
        return {
          match: async (url: string) => store.get(url),
          delete: async (url: string) => store.delete(url),
          keys: async () => [...store.keys()].map((url) => ({ url })),
          put: async () => undefined,
        };
      }
    })();
    await syncIdentity("u1", { caches, identity });
    expect([...caches.stores.keys()]).toEqual([]);
  });
});
