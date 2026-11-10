import type { Identity, IdentityStore } from "./offline";

const DB = "conta-verde-offline";
const STORE = "identity";
const KEY = "who";

/** Who is signed in on this device, kept in IndexedDB because a worker is stopped and started at will: the page writes it, the worker reads it. */
export function indexedDbIdentity(): IdentityStore {
  const open = () =>
    new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open(DB, 1);
      request.onupgradeneeded = () => request.result.createObjectStore(STORE);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });

  async function run<T>(mode: IDBTransactionMode, work: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {
    const db = await open();
    try {
      return await new Promise<T>((resolve, reject) => {
        const request = work(db.transaction(STORE, mode).objectStore(STORE));
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
    } finally {
      db.close();
    }
  }

  return {
    async get() {
      const found = await run<Identity | undefined>("readonly", (store) => store.get(KEY));
      return found ?? null;
    },
    async set(identity) {
      if (identity === null) await run("readwrite", (store) => store.delete(KEY));
      else await run("readwrite", (store) => store.put(identity, KEY));
    },
  };
}
