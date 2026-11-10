/**
 * @module k:/tests/collections.test.js
 *
 * `k.storage.BrowserCollections` (docs/design/52-indexed-collections.md; docs/compiler/runtime.md
 * §12.18) against a scripted IndexedDB written from the Indexed Database API 3.0 standard: an
 * `open` without a version opens the current one (creating version 1, with an upgrade, when the
 * database is new); an `open` at a higher version fires `versionchange` at every other open
 * connection, waits (`blocked`) until they close, then runs `upgradeneeded` in a `versionchange`
 * transaction, where stores and indexes are created and cursors may update records; an `open` at a
 * lower version fails with `VersionError`; key paths (`k`, `i.<field>`) extract keys, a record
 * whose index key path resolves to nothing isn't in that index; keys order numbers before text;
 * `IDBKeyRange.bound` with `lower > upper` throws `DataError`; and a transaction completes once its
 * requests have succeeded, or aborts (with `QuotaExceededError` when the origin is full).
 */
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { BrowserCollections } from "k:/runtime/collections.js";
import { unitCase } from "k:runtime";

const UNAVAILABLE = unitCase(0);
const SECOND = unitCase(1); // `unreadable` for reads, `quotaExceeded` for writes

/** Compares two IndexedDB keys: numbers before text. */
const cmp = (a, b) => (typeof a !== typeof b ? (typeof a === "number" ? -1 : 1) : a < b ? -1 : a > b ? 1 : 0);

/** The value at a key path (`i.title`), `undefined` when it resolves to nothing. */
const at = (value, path) => path.split(".").reduce((v, p) => (v == null ? undefined : v[p]), value);

const later = (f) => queueMicrotask(f);

/** A copy of a record, as IndexedDB's structured clone gives (records are JSON). */
const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));

/** A scripted `IDBKeyRange`. */
const KeyRange = {
  only: (k) => ({ lower: k, upper: k }),
  lowerBound: (k) => ({ lower: k, upper: undefined }),
  upperBound: (k) => ({ lower: undefined, upper: k }),
  bound: (lo, hi) => {
    if (cmp(lo, hi) > 0) throw Object.assign(new Error("The lower key is greater than the upper key."), { name: "DataError" });
    return { lower: lo, upper: hi };
  },
};
const inRange = (r, k) => r == null || ((r.lower === undefined || cmp(k, r.lower) >= 0) && (r.upper === undefined || cmp(k, r.upper) <= 0));

/**
 * A scripted `indexedDB`. `dbs` holds each database (`{version, stores}`, a store `{keyPath,
 * indexes: Map(name → keyPath), records: Map}`); `opens` records each `open(name, version)`;
 * `connections` the open ones; `fail` makes the next `open` fail; `quota` makes the next
 * `readwrite` transaction abort as full; `transactions` counts them by mode.
 */
function fakeIndexedDB() {
  const fake = { dbs: new Map(), opens: [], connections: [], fail: null, quota: false, transactions: { readonly: 0, readwrite: 0 }, blockedEvents: 0 };
  const names = (m) => ({ contains: (n) => m.has(n), get length() { return m.size; } });
  const sorted = (store) => [...store.records.values()].sort((a, b) => cmp(at(a, store.keyPath), at(b, store.keyPath)));
  function transaction(conn, storeNames, mode) {
    const t = { mode, pending: 0, done: false, error: null, oncomplete: null, onabort: null, onerror: null };
    const settle = () => {
      if (t.pending > 0 || t.done) return;
      later(() => {
        if (t.pending > 0 || t.done) return;
        t.done = true;
        if (mode === "readwrite" && fake.quota) {
          fake.quota = false;
          t.error = Object.assign(new Error("The quota has been exceeded."), { name: "QuotaExceededError" });
          // Nothing the transaction wrote is kept.
          for (const [s, copy] of t.snapshots) conn.db.stores.get(s).records = copy;
          t.onabort?.();
          return;
        }
        t.oncomplete?.();
      });
    };
    t.snapshots = new Map();
    const request = (compute) => {
      const r = { result: undefined, error: null, onsuccess: null, onerror: null };
      t.pending++;
      later(() => {
        r.result = compute();
        t.pending--;
        r.onsuccess?.({ target: r });
        settle();
      });
      return r;
    };
    const cursorOver = (list, value, store) => {
      const r = { result: undefined, onsuccess: null };
      let i = 0;
      const step = () => {
        t.pending++;
        later(() => {
          t.pending--;
          if (i >= list.length) r.result = null;
          else {
            const rec = list[i++];
            r.result = {
              value: value(rec),
              continue: step,
              update: (v) => store && store.records.set(JSON.stringify(at(v, store.keyPath)), clone(v)),
            };
          }
          r.onsuccess?.({ target: r });
          settle();
        });
      };
      step();
      return r;
    };
    t.objectStore = (name) => {
      if (!storeNames.includes(name) && mode !== "versionchange") throw Object.assign(new Error("not in scope"), { name: "NotFoundError" });
      const store = conn.db.stores.get(name);
      if (mode === "readwrite" && !t.snapshots.has(name)) t.snapshots.set(name, new Map(store.records));
      const key = (v) => JSON.stringify(v);
      return {
        get indexNames() {
          return names(store.indexes);
        },
        createIndex: (n, path) => store.indexes.set(n, path),
        get: (k) => request(() => clone(store.records.get(key(k)))),
        getAll: () => request(() => sorted(store).map((r) => clone(r))),
        count: () => request(() => store.records.size),
        put: (v) => request(() => void store.records.set(key(at(v, store.keyPath)), clone(v))),
        delete: (k) => request(() => void store.records.delete(key(k))),
        clear: () => request(() => void store.records.clear()),
        openCursor: () => cursorOver(sorted(store), (r) => clone(r), store),
        index: (n) => {
          const path = store.indexes.get(n);
          const entries = (range) =>
            sorted(store)
              .filter((r) => at(r, path) !== undefined && inRange(range, at(r, path)))
              .sort((a, b) => cmp(at(a, path), at(b, path)) || cmp(at(a, store.keyPath), at(b, store.keyPath)));
          return {
            getAll: (range) => request(() => entries(range).map((r) => clone(r))),
            openCursor: (range, direction) => {
              const list = entries(range);
              if (direction === "prev") list.reverse();
              return cursorOver(list, (r) => clone(r), null);
            },
          };
        },
      };
    };
    t.abort = () => {
      t.done = true;
      t.onabort?.();
    };
    if (mode !== "versionchange") fake.transactions[mode]++;
    settle();
    return t;
  }
  function connect(name, db) {
    const conn = {
      db,
      name,
      closed: false,
      onversionchange: null,
      get version() {
        return db.version;
      },
      get objectStoreNames() {
        return names(db.stores);
      },
      createObjectStore: (s, o) => {
        db.stores.set(s, { keyPath: o.keyPath, indexes: new Map(), records: new Map() });
        return conn.upgrading.objectStore(s);
      },
      transaction: (storeNames, mode) => {
        if (conn.closed) throw Object.assign(new Error("The database connection is closing."), { name: "InvalidStateError" });
        return transaction(conn, [storeNames].flat(), mode);
      },
      close: () => {
        conn.closed = true;
        fake.connections = fake.connections.filter((c) => c !== conn);
        fake.waiting.splice(0).forEach((f) => f());
      },
    };
    fake.connections.push(conn);
    return conn;
  }
  fake.waiting = [];
  fake.cmp = cmp;
  fake.open = (name, version) => {
    fake.opens.push([name, version]);
    const r = { result: null, error: null, transaction: null, onsuccess: null, onerror: null, onupgradeneeded: null, onblocked: null };
    later(async () => {
      if (fake.fail) {
        r.error = Object.assign(new Error("blocked"), { name: fake.fail });
        fake.fail = null;
        r.onerror?.();
        return;
      }
      const existing = fake.dbs.get(name);
      const current = existing?.version ?? 0;
      const target = version ?? Math.max(current, 1);
      if (target < current) {
        r.error = Object.assign(new Error("The requested version is less than the existing version."), { name: "VersionError" });
        r.onerror?.();
        return;
      }
      if (target > current) {
        // Others are told, and the upgrade waits until they have closed.
        const others = fake.connections.filter((c) => c.name === name);
        for (const c of others) c.onversionchange?.({ oldVersion: current, newVersion: target });
        while (fake.connections.some((c) => c.name === name)) {
          fake.blockedEvents++;
          r.onblocked?.();
          await new Promise((res) => fake.waiting.push(res));
        }
        const db = existing ?? { version: 0, stores: new Map() };
        fake.dbs.set(name, db);
        db.version = target;
        const conn = connect(name, db);
        r.result = conn;
        const t = transaction(conn, [], "versionchange");
        conn.upgrading = t;
        r.transaction = t;
        r.onupgradeneeded?.();
        await new Promise((res) => {
          t.oncomplete = res;
        });
        r.onsuccess?.();
        return;
      }
      r.result = connect(name, existing);
      r.onsuccess?.();
    });
    return r;
  };
  return fake;
}

/** Runs `f(fake)` with the scripted IndexedDB (and optionally `navigator`) installed. */
async function withIndexedDB(f, navigator) {
  const saved = { indexedDB: globalThis.indexedDB, IDBKeyRange: globalThis.IDBKeyRange, navigator: globalThis.navigator };
  const fake = fakeIndexedDB();
  globalThis.indexedDB = fake;
  globalThis.IDBKeyRange = KeyRange;
  if (navigator !== undefined) globalThis.navigator = navigator;
  try {
    await f(fake);
  } finally {
    for (const k of Object.keys(saved)) {
      if (saved[k] === undefined) delete globalThis[k];
      else globalThis[k] = saved[k];
    }
  }
}

/** A note's codec: `{id, title, at, tag}`, `title` a string. */
const NOTE = {
  encode: (v) => ({ id: v.id, title: v.title, at: v.at, tag: v.tag }),
  decode: (j) => {
    if (typeof j.title !== "string") throw new Error("expected a string");
    return { id: j.id, title: j.title, at: j.at, tag: j.tag ?? null };
  },
};
const ID = { encode: (v) => v };
const TEXT = { encode: (v) => v };
const notes = { name: "notes", key: "id" };
const byAt = { collection: "notes", field: "at" };
const byTag = { collection: "notes", field: "tag" };
// Every field of a key type is in each record's `i`, indexed or not.
const FIELDS = [["id", "id", "i"], ["at", "at", "d"], ["tag", "tag", "t"]];
const V1 = { notes: ["id", "i", [["at", "at", "d"]], FIELDS] };
const V2 = { notes: ["id", "i", [["at", "at", "d"], ["tag", "tag", "t"]], FIELDS] };
const note = (id, title, at, tag = null) => ({ id, title, at, tag });

test("a new database is created with a store per collection, keyed by k, indexed by i.<field>", () =>
  withIndexedDB(async (fake) => {
    const c = BrowserCollections("shop", V2);
    await c.put(null, ID, NOTE, note(1, "a", "2026-11-10T00:00:00Z", "x"), notes);
    const db = fake.dbs.get("k-collections:shop");
    assertEq(db.version, 1, "the first open created version 1");
    const store = db.stores.get("notes");
    assertEq(store.keyPath, "k");
    assertDeepEq([...store.indexes], [["at", "i.at"], ["tag", "i.tag"]]);
    const record = [...store.records.values()][0];
    assertDeepEq(record, { k: 1, i: { id: 1, at: Date.parse("2026-11-10T00:00:00Z"), tag: "x" }, v: '{"id":1,"title":"a","at":"2026-11-10T00:00:00Z","tag":"x"}' });
    assertDeepEq(await c.get(null, ID, NOTE, 1, notes), note(1, "a", "2026-11-10T00:00:00Z", "x"));
    assertEq(await c.get(null, ID, NOTE, 2, notes), null, "a missing key reads as nil");
    assertEq(await c.count(null, ID, NOTE, notes), 1);
  }));

test("all is in key order; find and range are in the field's order, without nil fields", () =>
  withIndexedDB(async (fake) => {
    const c = BrowserCollections("shop", V2);
    await c.putAll(null, ID, NOTE, [note(10, "ten", "2026-11-10T00:00:01Z", "x"), note(9, "nine", "2026-11-10T00:00:00.500Z"), note(2, "two", "2026-11-10T00:00:00Z", "x")], notes);
    assertEq(fake.transactions.readwrite, 1, "putAll is one transaction");
    assertDeepEq((await c.all(null, ID, NOTE, notes)).map((n) => n.id), [2, 9, 10]);
    // Milliseconds don't break the order, as the RFC 3339 text would.
    assertDeepEq((await c.range(null, TEXT, NOTE, byAt, undefined, undefined, undefined, undefined)).map((n) => n.id), [2, 9, 10]);
    assertDeepEq((await c.range(null, TEXT, NOTE, byAt, null, null, true, 2)).map((n) => n.id), [10, 9]);
    assertDeepEq((await c.range(null, TEXT, NOTE, byAt, "2026-11-10T00:00:00.001Z", "2026-11-10T00:00:01Z", false, null)).map((n) => n.id), [9, 10]);
    assertDeepEq(await c.range(null, TEXT, NOTE, byAt, "2026-11-11T00:00:00Z", "2026-11-10T00:00:00Z", false, null), [], "an empty range");
    assertDeepEq((await c.find(null, TEXT, NOTE, "x", byTag)).map((n) => n.id), [2, 10]);
    assertDeepEq((await c.range(null, TEXT, NOTE, byTag, null, null, false, null)).length, 2, "nil isn't indexed");
    await c.delete(null, ID, NOTE, 9, notes);
    await c.delete(null, ID, NOTE, 42, notes);
    assertEq(await c.count(null, ID, NOTE, notes), 2);
    await c.clear(null, ID, NOTE, notes);
    assertDeepEq(await c.all(null, ID, NOTE, notes), []);
  }));

test("an upgrade adds the missing index, fills it from the stored values, and deletes nothing", () =>
  withIndexedDB(async (fake) => {
    // A build whose value had no key-typed `tag` yet wrote no key for it.
    const old = BrowserCollections("shop", { notes: ["id", "i", [["at", "at", "d"]], [["at", "at", "d"]]], gone: ["id", "t", [], []] });
    await old.putAll(null, ID, NOTE, [note(1, "a", "2026-11-10T00:00:00Z", "x"), note(2, "b", "2026-11-10T00:00:01Z")], notes);
    assertEq(fake.dbs.get("k-collections:shop").version, 1);
    const fresh = BrowserCollections("shop", V2);
    assertDeepEq((await fresh.find(null, TEXT, NOTE, "x", byTag)).map((n) => n.id), [1], "the index found a value stored before it existed");
    const db = fake.dbs.get("k-collections:shop");
    assertEq(db.version, 2, "one upgrade, one version");
    assert(db.stores.has("gone"), "a store no longer declared is kept");
    assertDeepEq(fake.opens.slice(-2), [["k-collections:shop", undefined], ["k-collections:shop", 2]]);
    // A third tab of the new build opens without upgrading again.
    const again = BrowserCollections("shop", V2);
    assertEq(await again.count(null, ID, NOTE, notes), 2);
    assertEq(fake.dbs.get("k-collections:shop").version, 2);
  }));

test("an older tab gives way to a newer tab's upgrade, then reopens without upgrading", () =>
  withIndexedDB(async (fake) => {
    const older = BrowserCollections("shop", V1);
    await older.put(null, ID, NOTE, note(1, "a", "2026-11-10T00:00:00Z", "x"), notes);
    assertEq(fake.connections.length, 1);
    const newer = BrowserCollections("shop", V2);
    assertDeepEq((await newer.find(null, TEXT, NOTE, "x", byTag)).map((n) => n.id), [1]);
    assertEq(fake.blockedEvents, 0, "the older tab closed at once, so nothing was blocked");
    // The older tab's next call reopens at the newer version, a superset of what it needs.
    await older.put(null, ID, NOTE, note(2, "b", "2026-11-10T00:00:01Z", "x"), notes);
    assertEq(fake.dbs.get("k-collections:shop").version, 2);
    assertDeepEq((await newer.find(null, TEXT, NOTE, "x", byTag)).map((n) => n.id), [1, 2], "the older tab's write is indexed");
  }));

test("an upgrade waits for a connection that doesn't close, then proceeds", () =>
  withIndexedDB(async (fake) => {
    const old = BrowserCollections("shop", V1);
    await old.count(null, ID, NOTE, notes);
    // Another connection on the name that ignores `versionchange`.
    const stubborn = fake.connections[0];
    stubborn.onversionchange = null;
    const fresh = BrowserCollections("shop", V2);
    let done = false;
    const pending = fresh.count(null, ID, NOTE, notes).then((n) => {
      done = true;
      return n;
    });
    await tick();
    assert(!done, "blocked while the other connection is open");
    assert(fake.blockedEvents > 0);
    stubborn.close();
    assertEq(await pending, 0);
    assertEq(fake.dbs.get("k-collections:shop").version, 2);
  }));

test("failures are typed: unavailable, quota exceeded, unreadable", () =>
  withIndexedDB(async (fake) => {
    fake.fail = "UnknownError";
    const blocked = BrowserCollections("shop", V1);
    let thrown = null;
    await blocked.get(null, ID, NOTE, 1, notes).catch((e) => (thrown = e));
    assertEq(thrown, UNAVAILABLE, "a failed open is unavailable");
    thrown = null;
    await blocked.put(null, ID, NOTE, note(1, "a", "2026-11-10T00:00:00Z"), notes).catch((e) => (thrown = e));
    assertEq(thrown, null, "the next call opens again");
    fake.quota = true;
    await blocked.putAll(null, ID, NOTE, [note(2, "b", "2026-11-10T00:00:00Z"), note(3, "c", "2026-11-10T00:00:00Z")], notes).catch((e) => (thrown = e));
    assertEq(thrown, SECOND, "a full origin is quotaExceeded");
    assertEq(await blocked.count(null, ID, NOTE, notes), 1, "nothing of the failed putAll was kept");
    const store = fake.dbs.get("k-collections:shop").stores.get("notes");
    store.records.set("7", { k: 7, i: {}, v: '{"id":7,"title":3}' });
    thrown = null;
    await blocked.all(null, ID, NOTE, notes).catch((e) => (thrown = e));
    assertEq(thrown, SECOND, "a value that doesn't decode is unreadable");
    assert(store.records.has("7"), "and is left alone");
  }));

test("without IndexedDB every call is unavailable", async () => {
  const saved = globalThis.indexedDB;
  delete globalThis.indexedDB;
  try {
    const c = BrowserCollections("shop", V1);
    let thrown = null;
    await c.count(null, ID, NOTE, notes).catch((e) => (thrown = e));
    assertEq(thrown, UNAVAILABLE);
  } finally {
    if (saved !== undefined) globalThis.indexedDB = saved;
  }
});

test("persistence and the estimate come from navigator.storage, best effort without it", async () => {
  let asked = 0;
  const storage = { persisted: async () => false, persist: async () => (++asked, true), estimate: async () => ({ usage: 1024.4, quota: 2048 }) };
  await withIndexedDB(async () => {
    const c = BrowserCollections("shop", V1);
    assertEq(await c.persistence(null), unitCase(1));
    assertEq(await c.requestPersistence(null), unitCase(0));
    assertEq(asked, 1);
    assertDeepEq(await c.estimate(null), { usage: 1024, quota: 2048 });
  }, { storage });
  await withIndexedDB(async () => {
    const c = BrowserCollections("shop", V1);
    assertEq(await c.requestPersistence(null), unitCase(1));
    assertEq(await c.estimate(null), null);
  }, {});
});
