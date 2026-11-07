// The database half of repositories (docs/compiler/db.md §4–§9): column
// codecs, parameters, row decoding, relations loaded in one query, typed
// unique violations, and transactions with savepoints. The ops are faked
// here; `k_server`'s db tests and `tests/corpus/db` run the real database.
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import {
  Database,
  dbCount,
  dbFirst,
  dbInList,
  dbInclude,
  dbInsertAll,
  dbParams,
  dbQuery,
  dbSelect,
  dbSingle,
  dbWrite,
  dbc,
  likePattern,
  unloaded,
  unloadedMany,
} from "k:runtime/server";
import { Currency, KPanic, codecs, decimal, money, parseInstant, unitCase } from "k:runtime";

/** Fake ops recording every call; `rows` answers queries by statement id. */
function fakeOps(rows = {}, writes = {}) {
  const calls = [];
  let next = 7;
  return {
    calls,
    op_k_db_query: async (s, p, tx) => {
      calls.push(["query", s, p, tx]);
      if (s === 99) throw new Error("database: statement failed: no such table");
      return rows[s] ?? [];
    },
    op_k_db_execute: async (s, p, tx) => {
      calls.push(["execute", s, p, tx]);
      return writes[s] ?? { count: 1 };
    },
    op_k_db_begin: async () => {
      calls.push(["begin"]);
      return next++;
    },
    op_k_db_finish: async (tx, commit) => calls.push([commit ? "commit" : "rollback", tx]),
    op_k_db_savepoint: async (tx, depth, op) => calls.push([["savepoint", "release", "rollbackTo"][op], tx, depth]),
    op_k_db_ping: async (ms) => ms > 0,
  };
}

async function rejects(p) {
  try {
    await p;
  } catch (e) {
    return e;
  }
  throw new Error("expected a rejection");
}

function enc(c, v) {
  const out = [];
  c.enc(v, out);
  return out;
}

test("scalar codecs store the documented column values", () => {
  assertDeepEq(enc(dbc.uuid, "0b2e…"), ["0b2e…"]);
  assertDeepEq(enc(dbc.int, 42), [42]);
  assertDeepEq(enc(dbc.bool, true), [true]);
  assertDeepEq(enc(dbc.decimal, decimal("-12.340")), ["-12.340"]);
  // Dates always carry milliseconds, so stored text sorts like time.
  assertDeepEq(enc(dbc.date, parseInstant("2026-11-07T10:00:00Z")), ["2026-11-07T10:00:00.000Z"]);
  assertDeepEq(enc(dbc.money, money("19.99", Currency.eur)), ["19.99", "EUR"]);
});

test("scalar codecs read back what both databases return", () => {
  assertEq(dbc.bool.dec([1], 0), true, "SQLite 1");
  assertEq(dbc.bool.dec([0], 0), false, "SQLite 0");
  assertEq(dbc.bool.dec([true], 0), true, "Postgres");
  // Decimals read back normalized, as Postgres pads them to ten places.
  assertEq(dbc.decimal.dec(["10.50"], 0).toString(), "10.5");
  assertEq(dbc.decimal.dec(["10.5000000000"], 0).toString(), "10.5");
  assertEq(dbc.decimal.dec(["-0.0000000000"], 0).toString(), "0");
  assertEq(dbc.date.dec(["2026-11-07T10:00:00.250Z"], 0).ms, Date.UTC(2026, 10, 7, 10, 0, 0, 250));
  // Postgres numeric(19,4) pads the amount; char(3) may pad the code.
  const m = dbc.money.dec(["19.9900", "EUR"], 0);
  assertEq(m.amount(), "19.99");
  assertEq(dbc.money.dec(["5", "USD "], 0).toString(), "5.00 USD");
});

test("malformed column values panic instead of becoming wrong values", () => {
  for (const [c, row] of [
    [dbc.int, [1.5]],
    [dbc.int, ["1"]],
    [dbc.decimal, ["abc"]],
    [dbc.date, ["yesterday"]],
    [dbc.money, ["1", "XYZ"]],
    [dbc.money, [null, "EUR"]],
  ]) {
    let threw = null;
    try {
      c.dec(row, 0);
    } catch (e) {
      threw = e;
    }
    assert(threw instanceof KPanic, `${JSON.stringify(row)} panics`);
  }
});

test("enums are stored by case name and unknown names panic", () => {
  const Status = { draft: unitCase(0), published: unitCase(1) };
  const c = dbc.enumeration(Status, ["draft", "published"]);
  assertDeepEq(enc(c, Status.published), ["published"]);
  assertEq(c.dec(["draft"], 0), Status.draft);
  for (const bad of ["archived", "toString", "__proto__"]) {
    let threw = false;
    try {
      c.dec([bad], 0);
    } catch (e) {
      threw = e instanceof KPanic;
    }
    assert(threw, bad);
  }
});

test("optional, wrapped and JSON codecs compose", () => {
  const o = dbc.optional(dbc.money);
  assertDeepEq(enc(o, null), [null, null], "nil fills every column");
  assertEq(o.dec([null, null], 0), null);
  const AccountId = (value) => ({ value });
  const w = dbc.wrap(AccountId, "value", dbc.uuid);
  assertDeepEq(enc(w, AccountId("u-1")), ["u-1"]);
  assertDeepEq(w.dec(["u-1"], 0), { value: "u-1" });
  const j = dbc.json(codecs.list(codecs.int));
  assertDeepEq(enc(j, [1, 2]), ["[1,2]"]);
  assertDeepEq(j.dec(["[3]"], 0), [3]);
});

test("parameters flatten in order and lists are one JSON parameter", () => {
  assertDeepEq(
    dbParams([
      [dbc.uuid, "a"],
      [dbc.money, money("1", Currency.usd)],
      [dbc.optional(dbc.int), null],
    ]),
    ["a", "1.00", "USD", null],
  );
  assertEq(dbInList(dbc.uuid, ["a", "b\"c"]), '["a","b\\"c"]');
  assertEq(dbInList(dbc.int, new Set([1, 2])), "[1,2]");
});

test("LIKE patterns escape their wildcards like k_db", () => {
  assertEq(likePattern(0, "50%_off\\"), "%50\\%\\_off\\\\%");
  assertEq(likePattern(1, "ab"), "ab%");
  assertEq(likePattern(2, "ab"), "%ab");
  assertEq(likePattern(0, ""), "%%");
});

test("unloaded relations know their id and panic on anything else", () => {
  const product = unloaded("Order.product", "p-1");
  assertEq(product.id, "p-1");
  assertEq(product.then, undefined, "not a thenable, so it can be awaited around");
  let threw = null;
  try {
    product.name;
  } catch (e) {
    threw = e;
  }
  assert(threw instanceof KPanic && threw.message.includes("Order.product"), String(threw));
  const tags = unloadedMany("Note.tags");
  for (const use of [() => tags.length, () => tags.map((x) => x), () => [...tags]]) {
    let panicked = false;
    try {
      use();
    } catch (e) {
      panicked = e instanceof KPanic;
    }
    assert(panicked, String(use));
  }
  // Copying an entity (`var edited = current`) doesn't touch the relation.
  const note = { id: "n", tags };
  const copy = { ...note, title: "x" };
  assertEq(copy.tags, tags);
});

test("selects decode every row; first and single check the count", async () => {
  const o = fakeOps({ 1: [["a", 1], ["b", 2]], 2: [], 3: [["c", 3]] });
  const db = new Database(o);
  const decode = (r) => ({ id: r[0], n: r[1] });
  assertDeepEq(await dbSelect(db, 1, ["x"], decode), [{ id: "a", n: 1 }, { id: "b", n: 2 }]);
  assertEq(await dbFirst(db, 2, [], decode), null);
  assertDeepEq(await dbFirst(db, 3, [], decode), { id: "c", n: 3 });
  assertDeepEq(await dbSingle(db, 3, [], decode, "Note"), { id: "c", n: 3 });
  const none = await rejects(dbSingle(db, 2, [], decode, "Note"));
  assert(none instanceof KPanic && none.message.includes("none"), String(none));
  const several = await rejects(dbSingle(db, 1, [], decode, "Note"));
  assert(several instanceof KPanic && several.message.includes("several"), String(several));
  assertDeepEq(o.calls[0], ["query", 1, ["x"], 0], "no transaction: tx 0");
});

test("count reads the single value", async () => {
  const db = new Database(fakeOps({ 4: [[5]] }));
  assertEq(await dbCount(db, 4, []), 5);
});

test("a database failure is a panic, and so is a missing database", async () => {
  const db = new Database(fakeOps());
  const e = await rejects(dbSelect(db, 99, [], (r) => r));
  assert(e instanceof KPanic && e.message.includes("no such table"), String(e));
  const missing = await rejects(dbSelect(undefined, 1, [], (r) => r));
  assert(missing instanceof KPanic && missing.message.includes("DATABASE_URL"), String(missing));
  const bare = await rejects(new Database({}).rows(1, []));
  assert(bare instanceof KPanic, String(bare));
});

test("writes return their count; unique violations throw the typed case", async () => {
  const emailTaken = { $: 0 };
  const db = new Database(fakeOps({}, { 5: { count: 3 }, 6: { unique: "email" }, 8: { unique: "slug" } }));
  assertEq(await dbWrite(db, 5, [], null), 3);
  assertEq(await rejects(dbWrite(db, 6, [], { email: emailTaken })), emailTaken);
  const unmapped = await rejects(dbWrite(db, 8, [], { email: emailTaken }));
  assert(unmapped instanceof KPanic, "a violation without a case panics");
  const none = await rejects(dbWrite(db, 6, [], null));
  assert(none instanceof KPanic, "an entity without unique fields panics");
});

test("including a to-many relation runs one query and groups by key", async () => {
  // Tags of notes n1 and n2; n3 has none.
  const o = fakeOps({
    9: [
      ["t1", "n1", "work"],
      ["t2", "n2", "home"],
      ["t3", "n1", "ideas"],
    ],
  });
  const db = new Database(o);
  const notes = [{ id: "n1", tags: unloadedMany("Note.tags") }, { id: "n2" }, { id: "n3" }];
  await dbInclude(db, notes, {
    many: true,
    field: "tags",
    statement: 9,
    decode: (r) => ({ id: r[0], note: unloaded("Tag.note", r[1]), label: r[2] }),
    key: dbc.uuid,
    inverse: "note",
  });
  assertDeepEq(
    notes.map((n) => n.tags.map((t) => t.label)),
    [["work", "ideas"], ["home"], []],
  );
  assertEq(o.calls.length, 1, "one query for every row");
  assertDeepEq(o.calls[0][2], ['["n1","n2","n3"]']);
});

test("including a to-one relation asks for each distinct key once", async () => {
  const o = fakeOps({ 10: [["p1", "Mug"], ["p2", "Pen"]] });
  const db = new Database(o);
  const orders = [
    { id: "o1", product: unloaded("Order.product", "p1") },
    { id: "o2", product: unloaded("Order.product", "p2") },
    { id: "o3", product: unloaded("Order.product", "p1") },
  ];
  await dbInclude(db, orders, {
    many: false,
    field: "product",
    statement: 10,
    decode: (r) => ({ id: r[0], name: r[1] }),
    key: dbc.uuid,
  });
  assertDeepEq(orders.map((x) => x.product.name), ["Mug", "Pen", "Mug"]);
  assertDeepEq(o.calls[0][2], ['["p1","p2"]']);
  await dbInclude(db, [], { many: false, field: "product", statement: 10, decode: (r) => r, key: dbc.uuid });
  assertEq(o.calls.length, 1, "no rows, no query");
});

test("a missing related row panics", async () => {
  const db = new Database(fakeOps({ 10: [] }));
  const e = await rejects(
    dbInclude(db, [{ id: "o", product: unloaded("Order.product", "gone") }], {
      many: false,
      field: "product",
      statement: 10,
      decode: (r) => r,
      key: dbc.uuid,
    }),
  );
  assert(e instanceof KPanic, String(e));
});

test("a transaction binds its statements and commits the block's value", async () => {
  const o = fakeOps();
  const db = new Database(o);
  const v = await db.transaction(null, async () => {
    await db.write(1, []);
    return 42;
  });
  assertEq(v, 42);
  assertDeepEq(o.calls, [["begin"], ["execute", 1, [], 7], ["commit", 7]]);
  assertEq(db.tx, 0);
  await db.rows(2, []);
  assertEq(o.calls.at(-1)[3], 0, "statements after the commit run outside");
});

test("a throw rolls back and is rethrown; a panic too", async () => {
  const o = fakeOps();
  const db = new Database(o);
  const err = { $: 1 };
  assertEq(await rejects(db.transaction(null, async () => { throw err; })), err);
  assertDeepEq(o.calls.at(-1), ["rollback", 7]);
  const p = await rejects(db.transaction(null, async () => { throw new KPanic("boom"); }));
  assert(p instanceof KPanic);
  assertDeepEq(o.calls.at(-1), ["rollback", 8]);
  assertEq(db.tx, 0);
});

test("nested transactions are savepoints that roll back alone", async () => {
  const o = fakeOps();
  const db = new Database(o);
  const err = { $: 0 };
  const v = await db.transaction(null, async () => {
    await db.write(1, []);
    const inner = await rejects(db.transaction(null, async () => {
      await db.write(2, []);
      throw err;
    }));
    assertEq(inner, err);
    await db.transaction(null, async () => db.write(3, []));
    return "outer";
  });
  assertEq(v, "outer");
  assertDeepEq(o.calls, [
    ["begin"],
    ["execute", 1, [], 7],
    ["savepoint", 7, 1],
    ["execute", 2, [], 7],
    ["rollbackTo", 7, 1],
    ["savepoint", 7, 1],
    ["execute", 3, [], 7],
    ["release", 7, 1],
    ["commit", 7],
  ]);
});

test("ping never throws", async () => {
  assertEq(await new Database(fakeOps()).ping(null, 2000), true);
  assertEq(await new Database({}).ping(null, 2000), false);
  const failing = new Database({ op_k_db_ping: async () => { throw new Error("down"); } });
  assertEq(await failing.ping(null, 2000), false);
});

test("one awaited query decodes, includes and shapes its result", async () => {
  const o = fakeOps({ 1: [["n1"], ["n2"]], 2: [["t1", "n2", "x"]], 3: [] });
  const db = new Database(o);
  const decode = (r) => ({ id: r[0], tags: unloadedMany("Note.tags") });
  const tags = {
    many: true,
    field: "tags",
    statement: 2,
    decode: (r) => ({ id: r[0], note: unloaded("Tag.note", r[1]), label: r[2] }),
    key: dbc.uuid,
    inverse: "note",
  };
  const list = await dbQuery(db, 0, 1, [], decode, [tags], "Note");
  assertDeepEq(list.map((n) => n.tags.length), [0, 1]);
  const first = await dbQuery(db, 1, 1, [], decode, [tags], "Note");
  assertEq(first.id, "n1");
  assertEq(await dbQuery(db, 1, 3, [], decode, [tags], "Note"), null);
  const e = await rejects(dbQuery(db, 2, 3, [], decode, [], "Note"));
  assert(e instanceof KPanic, "single of nothing panics");
  assertEq(o.calls.filter((c) => c[1] === 2).length, 2, "no include query for a missing first");
});

test("insert(all:) splits rows over the rendered batch sizes in one transaction", async () => {
  const o = fakeOps();
  const db = new Database(o);
  const values = (e, out) => out.push(e.id, e.label);
  const rows = Array.from({ length: 11 }, (_, i) => ({ id: `t${i}`, label: "x" }));
  await dbInsertAll(db, [[8, 30], [4, 31], [1, 32]], values, rows, null);
  const writes = o.calls.filter((c) => c[0] === "execute");
  assertDeepEq(writes.map((c) => [c[1], c[2].length]), [[30, 16], [32, 2], [32, 2], [32, 2]]);
  assertEq(o.calls[0][0], "begin");
  assertEq(o.calls.at(-1)[0], "commit");
  // One statement needs no transaction; no rows need nothing.
  const one = fakeOps();
  await dbInsertAll(new Database(one), [[8, 30], [1, 32]], values, rows.slice(0, 1), null);
  assertDeepEq(one.calls.map((c) => c[0]), ["execute"]);
  const none = fakeOps();
  await dbInsertAll(new Database(none), [[1, 32]], values, [], null);
  assertEq(none.calls.length, 0);
});

test("a duplicate in insert(all:) rolls the whole insert back", async () => {
  const taken = { $: 0 };
  const o = fakeOps({}, { 32: { unique: "label" } });
  const db = new Database(o);
  const rows = [{ id: "a" }, { id: "b" }];
  const e = await rejects(dbInsertAll(db, [[1, 32]], (x, out) => out.push(x.id), rows, { label: taken }));
  assertEq(e, taken);
  assertEq(o.calls.at(-1)[0], "rollback");
});
