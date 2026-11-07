/**
 * @module k:/runtime/db.js
 *
 * The database half of generated repositories (docs/compiler/db.md §4–§9,
 * docs/design/19-persistence.md), exported through `k:runtime/server`.
 *
 * The SQL of every query was rendered at build time; generated code refers
 * to a statement only by its number and passes its parameters, already
 * encoded by the column codecs below. The Rust half (`k_server::db`) runs
 * it on the request's transaction or the shared connection.
 *
 * - **Column codecs** ({@link dbc}) convert between K values and column
 *   values: a `Date` is RFC 3339 UTC text with milliseconds, a `Decimal`
 *   its text, `Money` two columns (amount text, currency code), a
 *   payload-free enum its case name, a `Codable` value JSON text, a
 *   one-field struct (`AccountId`) its field's column.
 * - **{@link Database}** is the request's `k.db.Database`: `transaction`
 *   binds every repository call of the request to one transaction, nested
 *   blocks to savepoints; `ping` never throws.
 * - **Repository helpers** ({@link dbSelect}, {@link dbFirst}, …) run a
 *   statement and decode its rows; {@link dbInclude} loads a relation for
 *   every row with one more query.
 *
 * Infrastructure failures (a lost connection, a bad statement) are panics:
 * the calling code can't handle them (19-persistence.md, "Writes"). A
 * duplicate on a `@Unique` field is the one typed outcome: {@link dbWrite}
 * throws the entity's `UniqueViolation` case.
 */

import { KPanic } from "./values.js";
import { currencyCode, currencyParse, money, parseDecimal, parseInstant, stringify } from "./std.js";

const ops = globalThis.Deno?.core?.ops ?? {};

/**
 * A column codec: how one K value is stored.
 * @typedef {object} ColumnCodec
 * @property {number} w the number of columns (2 for money, else 1)
 * @property {(v: any, out: unknown[]) => void} enc appends the column values of `v`
 * @property {(row: unknown[], i: number) => any} dec reads the value stored from column `i`
 */

/** @param {unknown} v @returns {string} */
function textOf(v) {
  if (typeof v !== "string") throw new KPanic(`database: expected text, got ${typeof v}`);
  return v;
}

/** @type {ColumnCodec} */
const plain = { w: 1, enc: (v, out) => out.push(v), dec: (r, i) => r[i] };

/**
 * The column codecs, one per column type of db.md §3 plus combinators.
 */
export const dbc = Object.freeze({
  /** `Uuid`: lowercase canonical text. */
  uuid: plain,
  /** `String`, `Url`: text. */
  text: plain,
  /** `EmailAddress`: its text. */
  email: plain,
  /** `Int`: an integer column. */
  int: {
    w: 1,
    enc: (v, out) => out.push(v),
    dec: (r, i) => {
      const v = r[i];
      if (typeof v !== "number" || !Number.isSafeInteger(v)) throw new KPanic(`database: ${v} is not an Int`);
      return v;
    },
  },
  /** `Bool`: `boolean` on Postgres, 0 or 1 on SQLite. */
  bool: { w: 1, enc: (v, out) => out.push(v), dec: (r, i) => r[i] === true || r[i] === 1 },
  /**
   * `Decimal`: its text, exact on both databases. It reads back
   * normalized (no trailing fractional zeros), since Postgres'
   * `numeric(38,10)` pads every value to ten places: `10.0` is `10` on both.
   */
  decimal: {
    w: 1,
    enc: (v, out) => out.push(v.toString()),
    dec: (r, i) => (parseDecimal(textOf(r[i])) ?? panicValue("a decimal", r[i])).normalized(),
  },
  /** `Date`: RFC 3339 UTC with milliseconds, so text order is time order on SQLite. */
  date: {
    w: 1,
    enc: (v, out) => out.push(new Date(v.ms).toISOString()),
    dec: (r, i) => parseInstant(textOf(r[i])) ?? panicValue("a timestamp", r[i]),
  },
  /** `Money`: `<name>_amount` (decimal text) and `<name>_currency` (ISO code). */
  money: {
    w: 2,
    enc: (v, out) => out.push(v.amount(), currencyCode(v.currency)),
    dec: (r, i) => {
      const c = currencyParse(textOf(r[i + 1]).trim());
      if (!c) panicValue("a currency code", r[i + 1]);
      return money(textOf(r[i]), c);
    },
  },
  /**
   * A payload-free enum, stored by case name.
   * @param {Record<string, any>} cases the generated enum object, by case name
   * @param {string[]} names the case names by tag
   * @returns {ColumnCodec}
   */
  enumeration: (cases, names) => ({
    w: 1,
    enc: (v, out) => out.push(names[v.$]),
    dec: (r, i) => {
      const name = textOf(r[i]);
      if (!Object.prototype.hasOwnProperty.call(cases, name) || !names.includes(name)) panicValue("a case name", name);
      return cases[name];
    },
  }),
  /**
   * A `Codable` value as JSON text.
   * @param {{encode: Function, decode: Function}} codec
   * @returns {ColumnCodec}
   */
  json: (codec) => ({
    w: 1,
    enc: (v, out) => out.push(stringify(codec.encode(v))),
    dec: (r, i) => codec.decode(JSON.parse(textOf(r[i])), "column"),
  }),
  /**
   * `T?`: `NULL` for `nil` in every column of `c`.
   * @param {ColumnCodec} c
   * @returns {ColumnCodec}
   */
  optional: (c) => ({
    w: c.w,
    enc: (v, out) => {
      if (v === null || v === undefined) for (let k = 0; k < c.w; k++) out.push(null);
      else c.enc(v, out);
    },
    dec: (r, i) => (r[i] === null || r[i] === undefined ? null : c.dec(r, i)),
  }),
  /**
   * A struct with one stored field (`struct AccountId { let value: Uuid }`),
   * stored as that field.
   * @param {(v: any) => any} make the generated constructor
   * @param {string} field the field's name
   * @param {ColumnCodec} c the field's codec
   * @returns {ColumnCodec}
   */
  wrap: (make, field, c) => ({ w: c.w, enc: (v, out) => c.enc(v[field], out), dec: (r, i) => make(c.dec(r, i)) }),
  /**
   * A to-one relation, stored as the related entity's id (`note_id`). A
   * read gives a stand-in that knows only the id until `.including`
   * replaces it (see {@link unloaded}).
   * @param {ColumnCodec} id the related entity's id codec
   * @param {string} owner `Tag.note`, for the panic message
   * @returns {ColumnCodec}
   */
  ref: (id, owner) => ({ w: id.w, enc: (v, out) => id.enc(v.id, out), dec: (r, i) => unloaded(owner, id.dec(r, i)) }),
});

/** @returns {never} */
function panicValue(what, v) {
  throw new KPanic(`database: expected ${what}, found ${JSON.stringify(v)}`);
}

/**
 * Encodes parameters: `pairs` of codec and value, flattened in order.
 * @param {Array<[ColumnCodec, any]>} pairs
 * @returns {unknown[]}
 */
export function dbParams(pairs) {
  const out = [];
  for (const [c, v] of pairs) c.enc(v, out);
  return out;
}

/**
 * An `In` list parameter (`ids.contains(n.id)`): the elements' column
 * values as one JSON array text, which SQLite expands with `json_each` and
 * Postgres binds as an array.
 * @param {ColumnCodec} c the element codec (one column)
 * @param {Iterable<any>} values
 * @returns {string}
 */
export function dbInList(c, values) {
  const out = [];
  for (const v of values) c.enc(v, out);
  return JSON.stringify(out);
}

/** `\`, `%` and `_`: the characters a `LIKE` pattern escapes. */
const LIKE_SPECIAL = new RegExp("[\\\\%_]", "g");

/**
 * The bind value of a `LIKE` predicate (`contains`, `hasPrefix`,
 * `hasSuffix`): `\`, `%` and `_` escaped, then `%` where the predicate
 * needs it; matches `k_db::sql::like_pattern`.
 * @param {0 | 1 | 2} kind contains, prefix, suffix
 * @param {string} text
 * @returns {string}
 */
export function likePattern(kind, text) {
  const escaped = String(text).replace(LIKE_SPECIAL, (c) => "\\" + c);
  return kind === 0 ? `%${escaped}%` : kind === 1 ? `${escaped}%` : `%${escaped}`;
}

/**
 * The stand-in for a to-one relation that wasn't loaded: its `id` is
 * known (the foreign key); reading anything else panics, as reading an
 * unloaded relation must (19-persistence.md, "Rules").
 * @param {string} owner `Order.product`
 * @param {any} id
 * @returns {object}
 */
export function unloaded(owner, id) {
  return new Proxy(Object.freeze({ id }), {
    get(t, key) {
      if (key === "id") return t.id;
      if (typeof key === "symbol" || key === "then" || key === "toJSON") return undefined;
      throw new KPanic(`${owner} was not loaded; add \`.including { … }\` to the query`);
    },
  });
}

/**
 * The stand-in for a to-many relation that wasn't loaded: any use panics.
 * @param {string} owner `Note.tags`
 * @returns {unknown[]}
 */
export function unloadedMany(owner) {
  return new Proxy(Object.freeze([]), {
    get(_t, key) {
      if (typeof key === "symbol" && key !== Symbol.iterator) return undefined;
      if (key === "then" || key === "toJSON") return undefined;
      throw new KPanic(`${owner} was not loaded; add \`.including { … }\` to the query`);
    },
  });
}

/** @param {unknown} e @returns {never} */
function failed(e) {
  if (e instanceof KPanic) throw e;
  throw new KPanic(e instanceof Error ? e.message : String(e));
}

/**
 * The request's `k.db.Database`. Repository calls go through
 * {@link Database#rows} and {@link Database#write}, which run on the open
 * transaction, if any.
 */
export class Database {
  /**
   * @param {object} [o] the database ops (`Deno.core.ops`); tests pass fakes
   */
  constructor(o = ops) {
    /** The ops this database calls. */
    this.ops = o;
    /** The open transaction's id, 0 for none. */
    this.tx = 0;
    /** The nesting depth inside it (savepoints). */
    this.depth = 0;
    /**
     * Work to do once the open transaction commits (`InlineJobQueue`):
     * `{depth, run}`, dropped with the savepoint or transaction that
     * registered it when that rolls back.
     * @type {Array<{depth: number, run: () => Promise<void>}>}
     */
    this.onCommit = [];
  }

  /**
   * Runs `run` once the open transaction commits, or now outside one.
   * @param {() => Promise<void>} run
   */
  async afterCommit(run) {
    if (this.tx === 0) return run();
    this.onCommit.push({ depth: this.depth, run });
  }

  /**
   * Runs read statement `statement`.
   * @param {number} statement
   * @param {unknown[]} params
   * @returns {Promise<unknown[][]>}
   */
  async rows(statement, params) {
    if (!this.ops.op_k_db_query) throw new KPanic("database: this runtime has no database");
    try {
      return await this.ops.op_k_db_query(statement, params, this.tx);
    } catch (e) {
      failed(e);
    }
  }

  /**
   * Runs write statement `statement`.
   * @param {number} statement
   * @param {unknown[]} params
   * @returns {Promise<{count: number} | {unique: string}>}
   */
  async write(statement, params) {
    if (!this.ops.op_k_db_execute) throw new KPanic("database: this runtime has no database");
    try {
      return await this.ops.op_k_db_execute(statement, params, this.tx);
    } catch (e) {
      failed(e);
    }
  }

  /**
   * `database.transaction { … }`: the block's value, committed; a throw
   * (or a panic) rolls back and is rethrown. Inside another transaction of
   * the same request the block is a savepoint.
   * @template T
   * @param {AbortSignal | null} c
   * @param {(c: AbortSignal | null) => Promise<T>} block
   * @returns {Promise<T>}
   */
  async transaction(c, block) {
    if (this.tx !== 0) return this.savepoint(c, block);
    try {
      this.tx = await this.ops.op_k_db_begin();
    } catch (e) {
      failed(e);
    }
    const tx = this.tx;
    let value;
    try {
      value = await block(c);
    } catch (e) {
      this.tx = 0;
      this.depth = 0;
      this.onCommit = [];
      try {
        await this.ops.op_k_db_finish(tx, false);
      } catch {
        // The block's error matters more than a failed rollback; the
        // connection discards the transaction either way.
      }
      throw e;
    }
    this.tx = 0;
    this.depth = 0;
    const committed = this.onCommit;
    this.onCommit = [];
    try {
      await this.ops.op_k_db_finish(tx, true);
    } catch (e) {
      failed(e);
    }
    for (const w of committed) await w.run();
    return value;
  }

  /**
   * A nested `transaction`: `SAVEPOINT k_sp_<depth>`, released when the
   * block returns and rolled back to when it throws.
   * @template T
   * @param {AbortSignal | null} c
   * @param {(c: AbortSignal | null) => Promise<T>} block
   * @returns {Promise<T>}
   */
  async savepoint(c, block) {
    const tx = this.tx;
    const depth = ++this.depth;
    try {
      await this.ops.op_k_db_savepoint(tx, depth, 0);
    } catch (e) {
      this.depth--;
      failed(e);
    }
    let value;
    try {
      value = await block(c);
    } catch (e) {
      this.depth = depth - 1;
      this.onCommit = this.onCommit.filter((w) => w.depth < depth);
      try {
        await this.ops.op_k_db_savepoint(tx, depth, 2);
      } catch {
        // As in `transaction`: rethrow the block's error.
      }
      throw e;
    }
    this.depth = depth - 1;
    try {
      await this.ops.op_k_db_savepoint(tx, depth, 1);
    } catch (e) {
      failed(e);
    }
    return value;
  }

  /**
   * `database.ping(timeout:)`: `SELECT 1` within the timeout; never throws.
   * @param {AbortSignal | null} _c
   * @param {number} timeout milliseconds
   * @returns {Promise<boolean>}
   */
  async ping(_c, timeout) {
    if (!this.ops.op_k_db_ping) return false;
    try {
      return await this.ops.op_k_db_ping(timeout);
    } catch {
      return false;
    }
  }
}

/**
 * The request's `Database` (a fresh one per request container).
 * @returns {Database}
 */
export function database() {
  return new Database();
}

/** @param {Database | undefined} db @returns {Database} */
function need(db) {
  if (!db) throw new KPanic("database: the app has no database (is DATABASE_URL set?)");
  return db;
}

/**
 * Runs a select and decodes every row.
 * @template T
 * @param {Database} db
 * @param {number} statement
 * @param {unknown[]} params
 * @param {(row: unknown[]) => T} decode
 * @returns {Promise<T[]>}
 */
export async function dbSelect(db, statement, params, decode) {
  const rows = await need(db).rows(statement, params);
  const out = new Array(rows.length);
  for (let i = 0; i < rows.length; i++) out[i] = decode(rows[i]);
  return out;
}

/**
 * `first { … }`: the first row (the statement has `LIMIT 1`), or `nil`.
 * @template T
 * @param {Database} db
 * @param {number} statement
 * @param {unknown[]} params
 * @param {(row: unknown[]) => T} decode
 * @returns {Promise<T | null>}
 */
export async function dbFirst(db, statement, params, decode) {
  const rows = await need(db).rows(statement, params);
  return rows.length === 0 ? null : decode(rows[0]);
}

/**
 * `single { … }`: exactly one row (the statement has `LIMIT 2`); zero or
 * several panic, because the caller has just written it.
 * @template T
 * @param {Database} db
 * @param {number} statement
 * @param {unknown[]} params
 * @param {(row: unknown[]) => T} decode
 * @param {string} entity for the message
 * @returns {Promise<T>}
 */
export async function dbSingle(db, statement, params, decode, entity) {
  const rows = await need(db).rows(statement, params);
  if (rows.length !== 1) {
    throw new KPanic(`single: expected exactly one ${entity}, found ${rows.length === 0 ? "none" : "several"}`);
  }
  return decode(rows[0]);
}

/**
 * `count()`.
 * @param {Database} db
 * @param {number} statement
 * @param {unknown[]} params
 * @returns {Promise<number>}
 */
export async function dbCount(db, statement, params) {
  const rows = await need(db).rows(statement, params);
  return Number(rows[0]?.[0] ?? 0);
}

/**
 * A write: the rows affected. A duplicate on a `@Unique` field throws the
 * entity's `UniqueViolation` case from `unique` (by field); without one
 * (an entity with no unique fields, or a primary key) it panics.
 * @param {Database} db
 * @param {number} statement
 * @param {unknown[]} params
 * @param {Record<string, unknown> | null} unique
 * @returns {Promise<number>}
 */
export async function dbWrite(db, statement, params, unique) {
  const r = await need(db).write(statement, params);
  if ("unique" in r) {
    if (unique && Object.prototype.hasOwnProperty.call(unique, r.unique)) throw unique[r.unique];
    throw new KPanic(`database: unique violation on ${r.unique}`);
  }
  return r.count;
}

/**
 * The column value of `v` under single-column codec `c`, as a map key.
 * @param {ColumnCodec} c
 * @param {any} v
 * @returns {unknown}
 */
function key(c, v) {
  const out = [];
  c.enc(v, out);
  return out[0];
}

/**
 * A relation to load with {@link dbInclude}.
 * @typedef {object} Relation
 * @property {boolean} many to-many (`Note.tags`) or to-one (`Order.product`)
 * @property {string} field the relation field
 * @property {number} statement selects the related rows whose key is in the list parameter `?1`
 * @property {(row: unknown[]) => any} decode decodes a related row
 * @property {ColumnCodec} key the codec of the key both sides compare (the id of the "one" side)
 * @property {string} [inverse] to-many: the related entity's to-one field back to the row
 */

/**
 * `.including { x => x.relation }`: loads `relation` for every row with
 * one query, whatever the number of rows (db.md §8), and stores it in the
 * rows. To-many children keep the database's order.
 * @param {Database} db
 * @param {any[]} rows freshly decoded entities
 * @param {Relation} rel
 * @returns {Promise<void>}
 */
export async function dbInclude(db, rows, rel) {
  if (rows.length === 0) return;
  const keys = [];
  const seen = new Set();
  for (const r of rows) {
    const k = key(rel.key, rel.many ? r.id : r[rel.field].id);
    if (!seen.has(k)) {
      seen.add(k);
      keys.push(k);
    }
  }
  const related = await dbSelect(db, rel.statement, [JSON.stringify(keys)], rel.decode);
  if (rel.many) {
    const groups = new Map();
    for (const x of related) {
      const k = key(rel.key, x[rel.inverse].id);
      const g = groups.get(k);
      if (g) g.push(x);
      else groups.set(k, [x]);
    }
    for (const r of rows) r[rel.field] = groups.get(key(rel.key, r.id)) ?? [];
  } else {
    const byId = new Map();
    for (const x of related) byId.set(key(rel.key, x.id), x);
    for (const r of rows) {
      const x = byId.get(key(rel.key, r[rel.field].id));
      if (x === undefined) throw new KPanic(`database: ${rel.field} refers to a row that doesn't exist`);
      r[rel.field] = x;
    }
  }
}

/**
 * One awaited query: a select whose rows are decoded, the relations of
 * `.including` loaded (one query each), and the result shaped by `kind`.
 * Generated repositories call it once per awaited query, so a query is a
 * single expression wherever it appears.
 * @param {Database} db
 * @param {0 | 1 | 2} kind 0 a list, 1 `first` (`T?`), 2 `single` (`T`)
 * @param {number} statement
 * @param {unknown[]} params
 * @param {(row: unknown[]) => any} decode
 * @param {Relation[]} includes
 * @param {string} entity for messages
 * @returns {Promise<any>}
 */
export async function dbQuery(db, kind, statement, params, decode, includes, entity) {
  let rows;
  if (kind === 0) {
    rows = await dbSelect(db, statement, params, decode);
  } else if (kind === 1) {
    const r = await dbFirst(db, statement, params, decode);
    if (r === null) return null;
    rows = [r];
  } else {
    rows = [await dbSingle(db, statement, params, decode, entity)];
  }
  for (const rel of includes) await dbInclude(db, rows, rel);
  return kind === 0 ? rows : rows[0];
}

/**
 * `insert(all:)`: every entity in as few statements as possible. The
 * build renders multi-row inserts for a few fixed row counts (`batches`,
 * largest first, always including 1), since SQL never comes from
 * JavaScript; the rows are split greedily over them. More than one
 * statement runs in a transaction (a savepoint inside an open one), so
 * the insert is all or nothing.
 * @param {Database} db
 * @param {Array<[number, number]>} batches `[rows, statement]` pairs, largest first
 * @param {(e: any, out: unknown[]) => void} values appends an entity's column values
 * @param {any[]} entities
 * @param {Record<string, unknown> | null} unique
 * @returns {Promise<void>}
 */
export async function dbInsertAll(db, batches, values, entities, unique) {
  const plan = [];
  let i = 0;
  while (i < entities.length) {
    const [n, statement] = batches.find(([rows]) => rows <= entities.length - i) ?? batches[batches.length - 1];
    const params = [];
    for (let k = i; k < i + n; k++) values(entities[k], params);
    plan.push([statement, params]);
    i += n;
  }
  if (plan.length === 0) return;
  const run = async () => {
    for (const [statement, params] of plan) await dbWrite(db, statement, params, unique);
  };
  if (plan.length === 1) await run();
  else await need(db).transaction(null, run);
}
