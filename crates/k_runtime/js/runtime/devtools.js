/**
 * @module k:runtime/devtools
 *
 * DevTools custom formatters for K values (docs/design/13-tooling.md,
 * "Debugging"): with "Enable custom formatters" on in Chrome's (or
 * Firefox's) DevTools settings, the console, the Scope pane and hovers
 * show K values as K: a `@State` signal or `@Derived` value as its current
 * value, `nil`, enum cases with their payloads, structs field by field,
 * `Money`, `Decimal` and `Date` as text, `List`, `Map` and `Set` with
 * their K names.
 *
 * Only development clients ship this file (it is appended to the runtime
 * chunk of a `Profile::Development` app); release and test builds never
 * contain it, so it costs them nothing. It reads values without
 * subscribing (`peek`), so inspecting a signal never changes what an
 * effect tracks.
 */

/**
 * The K case name of a payload enum value built by a development client
 * (`{ $: tag, … }`), set as a non-enumerable property so equality and
 * JSON never see it.
 */
const DEVTOOLS_CASE = Symbol.for("k.case");

/** A JsonML element: `[tag, attributes, ...children]`. */
const dtStyle = {
  name: "color: #881391",
  key: "color: #881391",
  case: "color: #1a1aa6",
  text: "color: #c41a16",
  num: "color: #1c00cf",
  dim: "color: #888",
};

/**
 * @param {unknown} v
 * @returns {boolean}
 */
function dtIsSignal(v) {
  return (
    v !== null &&
    typeof v === "object" &&
    "node" in v &&
    typeof v.peek === "function" &&
    typeof v.get === "function"
  );
}

/**
 * The K kind of `v`, or `null` when it is a plain JavaScript value the
 * default formatting shows well (numbers, strings, functions, DOM nodes).
 * @param {unknown} v
 * @returns {string|null}
 */
function dtKindOf(v) {
  if (v === null || typeof v !== "object") return null;
  if (dtIsSignal(v)) return "signal";
  if (Array.isArray(v)) return "list";
  if (v instanceof Map || v?.constructor?.name === "HashMap") return "map";
  if (v instanceof Set || v?.constructor?.name === "HashSet") return "set";
  if (typeof v.minor === "bigint" && v.currency && typeof v.toString === "function") return "money";
  if (typeof v.units === "bigint" && typeof v.scale === "number") return "decimal";
  if (v.constructor?.name === "Instant" && typeof v.ms === "number") return "date";
  if ((typeof globalThis.Node === "function" && v instanceof globalThis.Node) || (typeof globalThis.Event === "function" && v instanceof globalThis.Event)) return null;
  const proto = Object.getPrototypeOf(v);
  if (proto !== Object.prototype && proto !== null) return null;
  if (typeof v.$ === "number") return "enum";
  return "struct";
}

/**
 * A value inline: a nested devtoolsFormatter object for K values, the value
 * itself (DevTools' own formatting) otherwise.
 * @param {unknown} v
 * @returns {unknown}
 */
function dtRef(v) {
  if (v === null) return ["span", { style: dtStyle.dim }, "nil"];
  if (typeof v === "string") return ["span", { style: dtStyle.text }, JSON.stringify(v)];
  if (typeof v === "number" || typeof v === "boolean") return ["span", { style: dtStyle.num }, String(v)];
  return ["object", { object: v }];
}

/**
 * The dtFields of a struct or an enum payload, without the tag.
 * @param {object} v
 * @returns {[string, unknown][]}
 */
function dtFields(v) {
  return Object.entries(v).filter(([k]) => k !== "$");
}

/**
 * A dtRow of the expanded body: `key: value`.
 * @param {string} key
 * @param {unknown} value
 */
function dtRow(key, value) {
  return ["div", { style: "margin-left: 14px" }, ["span", { style: dtStyle.key }, key], ": ", dtRef(value)];
}

/**
 * The one-line dtHeader of a K value.
 * @param {unknown} v
 * @param {string} kind
 */
function dtHeader(v, kind) {
  switch (kind) {
    case "signal": {
      const current = v.peek();
      return ["span", {}, ["span", { style: dtStyle.dim }, v.constructor.name === "Computed" ? "@Derived " : "@State "], dtRef(current)];
    }
    case "list":
      return ["span", {}, ["span", { style: dtStyle.name }, `List(${v.length})`], ` [${v.slice(0, 5).map(dtShort).join(", ")}${v.length > 5 ? ", …" : ""}]`];
    case "map":
      return ["span", { style: dtStyle.name }, `Map(${v.size})`];
    case "set":
      return ["span", { style: dtStyle.name }, `Set(${v.size})`];
    case "money":
    case "decimal":
      return ["span", {}, ["span", { style: dtStyle.name }, kind === "money" ? "Money " : "Decimal "], ["span", { style: dtStyle.num }, String(v)]];
    case "date":
      return ["span", {}, ["span", { style: dtStyle.name }, "Date "], ["span", { style: dtStyle.num }, new Date(v.ms).toISOString()]];
    case "enum": {
      const name = v[DEVTOOLS_CASE] ?? `#${v.$}`;
      const fs = dtFields(v);
      if (fs.length === 0) return ["span", { style: dtStyle.case }, `.${name}`];
      return ["span", {}, ["span", { style: dtStyle.case }, `.${name}`], `(${fs.map(([k, x]) => `${k}: ${dtShort(x)}`).join(", ")})`];
    }
    default: {
      const fs = dtFields(v);
      return ["span", {}, ["span", { style: dtStyle.name }, "struct"], ` { ${fs.slice(0, 4).map(([k, x]) => `${k}: ${dtShort(x)}`).join(", ")}${fs.length > 4 ? ", …" : ""} }`];
    }
  }
}

/**
 * A dtShort text form for headers.
 * @param {unknown} v
 * @returns {string}
 */
function dtShort(v) {
  if (v === null || v === undefined) return "nil";
  if (typeof v === "string") return JSON.stringify(v);
  if (typeof v !== "object") return String(v);
  const kind = dtKindOf(v);
  if (kind === "signal") return dtShort(v.peek());
  if (kind === "list") return `List(${v.length})`;
  if (kind === "money" || kind === "decimal") return String(v);
  if (kind === "date") return new Date(v.ms).toISOString();
  if (kind === "enum") return `.${v[DEVTOOLS_CASE] ?? `#${v.$}`}`;
  if (kind === "map" || kind === "set") return `${kind === "map" ? "Map" : "Set"}(${v.size})`;
  return "{…}";
}

/** The devtoolsFormatter DevTools calls for every value it shows. */
export const devtoolsFormatter = {
  header(v, config) {
    if (config && config.k) return null;
    const kind = dtKindOf(v);
    if (kind === null) return null;
    try {
      return dtHeader(v, kind);
    } catch {
      return null;
    }
  },
  hasBody(v) {
    const kind = dtKindOf(v);
    if (kind === "signal") {
      const c = v.peek();
      return c !== null && typeof c === "object";
    }
    if (kind === "list") return v.length > 0;
    if (kind === "map" || kind === "set") return v.size > 0;
    if (kind === "enum" || kind === "struct") return dtFields(v).length > 0;
    return false;
  },
  body(v) {
    const kind = dtKindOf(v);
    const rows = [];
    if (kind === "signal") rows.push(dtRow("value", v.peek()));
    else if (kind === "list") v.forEach((x, i) => rows.push(dtRow(String(i), x)));
    else if (kind === "map") for (const [k, x] of v.entries()) rows.push(dtRow(dtShort(k), x));
    else if (kind === "set") for (const x of v.values()) rows.push(["div", { style: "margin-left: 14px" }, dtRef(x)]);
    else for (const [k, x] of dtFields(v)) rows.push(dtRow(k, x));
    return ["div", {}, ...rows];
  },
};

/**
 * Names a payload case built in a development client: `{ $: 1, … }`
 * becomes `.outOfStock(available: 3)` in DevTools. Returns the value.
 * @template T
 * @param {T} value
 * @param {string} name the case's K name
 * @returns {T}
 */
export function devCase(value, name) {
  Object.defineProperty(value, DEVTOOLS_CASE, { value: name, enumerable: false });
  return value;
}

/** Installs the devtoolsFormatter once per page. */
export function installFormatters() {
  if (typeof globalThis === "undefined") return;
  const list = (globalThis.devtoolsFormatters ??= []);
  if (!list.includes(devtoolsFormatter)) list.push(devtoolsFormatter);
}

installFormatters();
