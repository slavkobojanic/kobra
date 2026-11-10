/**
 * @module k:runtime/collections
 *
 * `k.storage.BrowserCollections` in the browser
 * (docs/design/52-indexed-collections.md; docs/compiler/runtime.md
 * §12.18): typed collections in IndexedDB, one database per app,
 * `k-collections:<namespace>`, with one object store per collection.
 *
 * The schema is the compiler's: `{name: [keyJson, keyKind, indexes,
 * fields]}`, a collection's key field, its indexes and every field of the
 * value with a key type, each as `[field, json, kind]`: its name, its key
 * in the value's JSON form and its kind (`t` text, `i` int, `d` date). A
 * stored record is `{k, i, v}`: `v` the value's JSON text, `k` the key
 * field's IndexedDB key and `i` the key of every such field (absent for
 * `nil`), indexed or not, so an index another build adds finds what this
 * build writes. The store's key path is `k`, an index's `i.<field>`, so no
 * declaration ever changes a key path. A `Date` key is its epoch
 * milliseconds, so it orders chronologically.
 *
 * The database is opened without a version and compared with the schema;
 * when a store or index is missing it is reopened at the next version,
 * whose upgrade only adds: stores, indexes, and the new indexes' keys of
 * the records already stored. Every connection closes when another tab
 * asks for a newer version, and the next call reopens it.
 *
 * `StorageReadError` is `unitCase(0)` `unavailable`, `1` `unreadable`;
 * `StorageWriteError` `0` `unavailable`, `1` `quotaExceeded`;
 * `StoragePersistence` `0` `persistent`, `1` `bestEffort`.
 *
 * Shipped as its own client chunk, only to apps whose client injects
 * `Collections` (codegen.md §16), and measured against its own size
 * budget. `k.test.MemoryCollections` (`testing/kit.js`) reuses the
 * record functions, so a test orders and decodes as the browser does.
 */

import { isPanic, unitCase } from "./values.js";

/**
 * The IndexedDB key of a field's JSON value, `undefined` for `nil`.
 * @param {string} kind `t`, `i` or `d`
 * @param {unknown} json
 * @returns {unknown}
 */
export const collectionKey = (kind, json) => (json == null ? undefined : kind === "d" ? Date.parse(String(json)) : json);

/**
 * The record of a value's JSON text, or `null` when the text isn't JSON
 * of an object with a key.
 * @param {Array} entry the collection's schema entry
 * @param {string} text
 * @returns {{k: unknown, i: object, v: string} | null}
 */
export function collectionRecord(entry, text) {
  let j;
  try {
    j = JSON.parse(text);
  } catch {
    return null;
  }
  if (j === null || typeof j !== "object") return null;
  const k = collectionKey(entry[1], j[entry[0]]);
  if (k === undefined) return null;
  const i = {};
  for (const [f, fj, fk] of entry[3] ?? entry[2]) {
    const x = collectionKey(fk, j[fj]);
    if (x !== undefined) i[f] = x;
  }
  return { k, i, v: text };
}

/**
 * Decodes the records' values with `codec`, throwing `unreadable` when
 * one doesn't decode.
 * @param {object} codec
 * @param {Array<{v: string}>} records
 * @param {unknown} unreadable
 * @returns {Array}
 */
export function collectionValues(codec, records, unreadable) {
  return records.map((r) => {
    try {
      return codec.decode(JSON.parse(r.v), "");
    } catch (e) {
      if (isPanic(e)) throw e;
      throw unreadable;
    }
  });
}

/**
 * The index of `index` in `schema`: `[storeName, field, kind]`, or `null`.
 * @param {object} schema
 * @param {{collection: string, field: string}} index
 */
export function collectionIndex(schema, index) {
  const e = schema[index.collection];
  const f = e && e[2].find((x) => x[0] === index.field);
  return f ? [index.collection, f[0], f[2]] : null;
}

/** `StorageReadError.unavailable`, and `StorageWriteError.unavailable`. */
const COL_UNAVAILABLE = unitCase(0);
/** `StorageReadError.unreadable`, and `StorageWriteError.quotaExceeded`. */
const COL_SECOND = unitCase(1);

/**
 * The browser's `k.storage.BrowserCollections` over IndexedDB.
 * @param {string} namespace the app's storage namespace
 * @param {object} schema the collections, as above
 * @returns {object}
 */
export function BrowserCollections(namespace, schema) {
  const g = globalThis;
  const name = `k-collections:${namespace}`;
  const stores = Object.keys(schema);
  let db = null;
  // Each store missing from `d`, or the fields of its indexes missing.
  const missing = (d) => {
    const out = [];
    const present = stores.filter((s) => d.objectStoreNames.contains(s));
    const t = present.length ? d.transaction(present, "readonly") : null;
    for (const s of stores) {
      if (!present.includes(s)) out.push(s);
      else if (schema[s][2].some(([f]) => !t.objectStore(s).indexNames.contains(f))) out.push(s);
    }
    return out;
  };
  // The upgrade: adds stores and indexes, and the new indexes' keys.
  const upgrade = (d, t) => {
    for (const s of stores) {
      const e = schema[s];
      const store = d.objectStoreNames.contains(s) ? t.objectStore(s) : d.createObjectStore(s, { keyPath: "k" });
      const added = e[2].filter(([f]) => !store.indexNames.contains(f));
      for (const [f] of added) store.createIndex(f, `i.${f}`);
      if (added.length === 0) continue;
      store.openCursor().onsuccess = (ev) => {
        const c = ev.target.result;
        if (!c) return;
        const r = collectionRecord(e, c.value.v);
        if (r) c.update({ ...c.value, i: { ...c.value.i, ...r.i } });
        c.continue();
      };
    }
  };
  const openAt = (version) =>
    new Promise((resolve, reject) => {
      const r = version === undefined ? g.indexedDB.open(name) : g.indexedDB.open(name, version);
      r.onupgradeneeded = () => upgrade(r.result, r.transaction);
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    });
  const connect = async () => {
    let d = await openAt();
    for (let n = 0; n < 8 && missing(d).length; n++) {
      const v = d.version + 1;
      d.close();
      // Another tab's upgrade to `v` may have come first.
      d = await openAt(v).catch((e) => (e?.name === "VersionError" ? openAt() : Promise.reject(e)));
    }
    if (missing(d).length) {
      d.close();
      throw COL_UNAVAILABLE;
    }
    // Gives way to another tab's upgrade; the next call reopens.
    d.onversionchange = () => {
      d.close();
      db = null;
    };
    return d;
  };
  const open = () =>
    (db ??= connect().catch(() => {
      db = null;
      throw COL_UNAVAILABLE;
    }));
  // Runs `f(store)` in one transaction on `s`, resolving with what the
  // getter it returns gives once the transaction completes.
  const run = async (s, mode, f, retry = true) => {
    const d = await open();
    let t;
    try {
      t = d.transaction(s, mode);
    } catch {
      // Closed by another tab's upgrade in the meantime: reopen once.
      db = null;
      if (retry) return run(s, mode, f, false);
      throw COL_UNAVAILABLE;
    }
    return new Promise((resolve, reject) => {
      const get = f(t.objectStore(s));
      t.oncomplete = () => resolve(get());
      t.onabort = () => reject(t.error?.name === "QuotaExceededError" ? COL_SECOND : COL_UNAVAILABLE);
    });
  };
  const one = (q) => () => q.result;
  const read = async (codec, s, f) => collectionValues(codec, await run(s, "readonly", f), COL_SECOND);
  const write = (s, f) => run(s, "readwrite", (store) => (f(store), () => {}));
  const key = (c, ck, k) => collectionKey(schema[c.name][1], ck.encode(k));
  const record = (c, cv, v) => collectionRecord(schema[c.name], JSON.stringify(cv.encode(v)));
  const persist = async (f) => {
    try {
      return unitCase((await f(g.navigator.storage)) ? 0 : 1);
    } catch {
      return unitCase(1);
    }
  };
  return {
    async get(_c, ck, cv, k, c) {
      const r = await run(c.name, "readonly", (store) => one(store.get(key(c, ck, k))));
      return r ? collectionValues(cv, [r], COL_SECOND)[0] : null;
    },
    all(_c, _ck, cv, c) {
      return read(cv, c.name, (store) => one(store.getAll()));
    },
    count(_c, _ck, _cv, c) {
      return run(c.name, "readonly", (store) => one(store.count()));
    },
    find(_c, cf, cv, value, index) {
      const [s, f, kind] = collectionIndex(schema, index);
      return read(cv, s, (store) => one(store.index(f).getAll(g.IDBKeyRange.only(collectionKey(kind, cf.encode(value))))));
    },
    async range(_c, cf, cv, index, from, to, reversed, limit) {
      const [s, f, kind] = collectionIndex(schema, index);
      const lo = from == null ? undefined : collectionKey(kind, cf.encode(from));
      const hi = to == null ? undefined : collectionKey(kind, cf.encode(to));
      const R = g.IDBKeyRange;
      // An empty range, which `IDBKeyRange.bound` refuses.
      if (lo !== undefined && hi !== undefined && g.indexedDB.cmp(lo, hi) > 0) return [];
      return read(cv, s, (store) => {
        const out = [];
        const range = lo === undefined ? (hi === undefined ? null : R.upperBound(hi)) : hi === undefined ? R.lowerBound(lo) : R.bound(lo, hi);
        const q = store.index(f).openCursor(range, reversed ? "prev" : "next");
        q.onsuccess = () => {
          const cur = q.result;
          if (cur && (limit == null || out.length < limit)) {
            out.push(cur.value);
            cur.continue();
          }
        };
        return () => out;
      });
    },
    put(_c, _ck, cv, value, c) {
      const r = record(c, cv, value);
      return write(c.name, (store) => store.put(r));
    },
    putAll(_c, _ck, cv, values, c) {
      const rs = values.map((v) => record(c, cv, v));
      return write(c.name, (store) => rs.forEach((r) => store.put(r)));
    },
    delete(_c, ck, _cv, k, c) {
      return write(c.name, (store) => store.delete(key(c, ck, k)));
    },
    clear(_c, _ck, _cv, c) {
      return write(c.name, (store) => store.clear());
    },
    persistence(_c) {
      return persist((s) => s.persisted());
    },
    requestPersistence(_c) {
      return persist((s) => s.persist());
    },
    async estimate(_c) {
      try {
        const e = await g.navigator.storage.estimate();
        return { usage: Math.round(e.usage ?? 0), quota: Math.round(e.quota ?? 0) };
      } catch {
        return null;
      }
    },
  };
}
