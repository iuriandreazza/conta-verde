import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearOfflineData, isOfflineEnabled, setOfflineEnabled, syncIdentity } from "./client";
import { dataCacheName, type CacheStorageLike, type Identity, type IdentityStore } from "./offline";

const names = new Set<string>();
const fakeCaches: CacheStorageLike = {
  async open() {
    throw new Error("not used");
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
