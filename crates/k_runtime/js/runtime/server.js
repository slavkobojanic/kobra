/**
 * @module k:runtime/server
 *
 * The JavaScript half of the server runtime (docs/compiler/server.md): the
 * request loop a worker isolate runs, plus SSR exports. The Rust half
 * (`k_server`) accepts connections, enforces body limits and RPC
 * preconditions, serves static assets, and hands every other request to
 * the loop through the `op_k_next_request` / `op_k_respond` ops.
 */

export { CLOSE, OPEN, Out, SEPARATOR, escapeAttribute, escapeText, serializeData } from "./ssr.js";
export { KPanic, isPanic } from "./values.js";
export {
  Metadata,
  RequestState,
  STREAMED,
  ServerBroadcast,
  checkGuards,
  clock,
  config,
  configEnum,
  configSecret,
  createApp,
  header,
  headerAll,
  info,
  missingNative,
  natives,
  param,
  parseCookies,
  parseQuery,
  publicUrls,
  redirect,
  requestJson,
  requestText,
  response,
  responseBody,
  responseHeaders,
  route,
  session,
  sessionManager,
  shareState,
  splitTarget,
} from "./app.js";

export {
  Database,
  database,
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
} from "./db.js";

import { STREAMED } from "./app.js";

/**
 * A request as the Rust front hands it over.
 * @typedef {object} Request
 * @property {number} id
 * @property {string} method upper case
 * @property {string} path the path and query, as received
 * @property {Array<[string, string]>} headers names lower case, in order
 * @property {Uint8Array} body at most the configured limit
 */

/**
 * A response for the Rust front.
 * @typedef {object} Response
 * @property {number} status
 * @property {Array<[string, string]>} headers
 * @property {string|Uint8Array} body
 */

const ops = globalThis.Deno?.core?.ops;

/**
 * Reads a configuration variable (`@Env`) at startup; `null` when unset.
 * @param {string} name
 * @returns {string|null}
 */
export function env(name) {
  const read = ops?.op_k_env;
  return typeof read === "function" ? read(name) : null;
}

/**
 * Serves requests until the server shuts down: pulls each request from the
 * Rust front, calls `handler` without waiting for earlier requests to
 * finish, and sends its response back. A handler that throws (a panic)
 * answers 500 with no details; the error is logged.
 * @param {(request: Request) => Promise<Response>|Response} handler
 * @returns {Promise<void>} resolves once the front closes the queue
 */
export async function serve(handler) {
  for (;;) {
    const request = await ops.op_k_next_request();
    if (request === null) return;
    handleOne(handler, request);
  }
}

const INTERNAL_ERROR = Object.freeze({
  status: 500,
  headers: [["content-type", "text/plain; charset=utf-8"]],
  body: "internal error",
});

/**
 * Whether `value` has the shape of a {@link Response}. Anything else from a
 * handler is a broken handler, answered with a 500.
 * @param {unknown} value
 * @returns {value is Response}
 */
function isResponse(value) {
  if (value === null || typeof value !== "object") return false;
  const r = /** @type {Record<string, unknown>} */ (value);
  return (
    typeof r.status === "number" &&
    Array.isArray(r.headers) &&
    r.headers.every((h) => Array.isArray(h) && h.length === 2 && typeof h[0] === "string" && typeof h[1] === "string") &&
    (typeof r.body === "string" || r.body instanceof Uint8Array)
  );
}

/**
 * Answers one request. Whatever the handler does (throw a non-Error,
 * reject, return something that isn't a response) costs this request a
 * 500 whose body says nothing about the failure; the worker's loop goes on.
 * @param {(request: Request) => Promise<Response>|Response} handler
 * @param {Request} request
 */
async function handleOne(handler, request) {
  let response;
  try {
    response = await handler(request);
    // A streamed rpc answers through the streaming ops itself.
    if (response === STREAMED) return;
    if (!isResponse(response)) {
      ops.op_k_log("error", `request ${request.id}: the handler returned something that is not a response`);
      response = INTERNAL_ERROR;
    }
  } catch (error) {
    ops.op_k_log("error", `request ${request.id} failed: ${describe(error)}`);
    response = INTERNAL_ERROR;
  }
  try {
    ops.op_k_respond(request.id, response);
  } catch (error) {
    ops.op_k_log("error", `request ${request.id}: the response was refused: ${describe(error)}`);
    try {
      ops.op_k_respond(request.id, INTERNAL_ERROR);
    } catch {
      // The request is already answered or gone.
    }
  }
}

/**
 * A printable description of a thrown value, for the log only.
 * @param {unknown} error
 * @returns {string}
 */
function describe(error) {
  try {
    return String(error?.stack ?? error);
  } catch {
    return "an unprintable value";
  }
}
