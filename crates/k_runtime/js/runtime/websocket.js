/**
 * @module k:runtime/websocket
 *
 * `k.http`'s raw WebSockets (docs/design/44-channels.md, "Raw WebSockets:
 * `k.http`"; runtime.md §12.13): the `WebSocket` value a `WebSocketClient`
 * opens ({@link openedSocket}, the same on both sides over a "port"), the
 * browser's `StandardWebSocketClient` over the browser's `WebSocket`, and the
 * `k.test` doubles `TestWebSocketClient` and `TestWebSocketServer`, which
 * the kit and the server of `k test` share (one per test).
 *
 * Values follow the generated shapes: a `WebSocketRequest` is a struct with
 * `url` (text), `subprotocol` (text or `null`), `headers`, `secretHeaders`,
 * `bearerToken` (server-only), `timeout` and `maxMessage`; a
 * `WebSocketMessage` is `{$: 0, _0: text}` or `{$: 1, _0: Uint8Array}`. The
 * functions receive the generated enums they build values of: `E`
 * (`WebSocketError`), `M` (`WebSocketMessage`) and `St` (`WebSocketState`).
 *
 * In the browser this file is a chunk of its own, shipped only to apps
 * whose client injects `WebSocketClient` (codegen.md §17), and measured
 * against its own size budget; the server imports it from `outbound.js`.
 */

import { signal } from "./signals.js";
import { PushStream } from "./std.js";
import { KPanic } from "./values.js";

/** How much may wait in the browser's send buffer before `send` waits: 1 MB. */
const SEND_BUFFER = 1048576;

/**
 * A K `WebSocket` over `port`, open: `port.send(m)` transmits a message
 * (and may return a promise), `port.close(code, reason)` starts closing and
 * `port.buffered()` is what still waits to go out. The transport reports
 * what happens with the returned `message(m)`, `closed(code, reason)` and
 * `tooLarge()`. When `owner` (the opening run's signal) aborts, the socket
 * closes.
 * @param {{E: object, St: object}} T the generated enums
 * @param {{send: Function, close: Function, buffered: () => number}} port
 * @param {string|null} protocol the subprotocol the endpoint chose
 * @param {AbortSignal|null} owner
 */
export function openedSocket(T, port, protocol, owner) {
  const { E, St } = T;
  const state = signal(St.open);
  const messages = new PushStream();
  let ended = false;
  let why = null;
  const closed = (code, reason) => {
    if (ended) return;
    ended = true;
    owner?.removeEventListener?.("abort", abort);
    state.set(St.closed(code, String(reason ?? "")));
    messages.push(null);
  };
  const socket = {
    /** `open`, `closing` or `closed(code:reason:)`; reactive. */
    get state() {
      return state.get();
    },
    subprotocol: protocol || null,
    messages,
    /**
     * Sends `m`, waiting while more than 1 MB is queued; throws `closed`,
     * or `tooLarge` after an oversized incoming message closed the socket.
     */
    async send(_c, m) {
      for (;;) {
        if (why !== null || state.peek().$ !== 0) throw why ?? E.closed;
        if (port.buffered() <= SEND_BUFFER) break;
        await new Promise((r) => setTimeout(r, 25));
      }
      if ((await port.send(m)) === false) throw why ?? E.closed;
    },
    /** Starts closing with `code` and `reason`; idempotent. */
    close(code, reason) {
      if (state.peek().$ !== 0) return;
      state.set(St.closing);
      port.close(code ?? 1000, reason ?? "");
    },
  };
  const abort = () => socket.close(1000, "");
  owner?.addEventListener?.("abort", abort);
  return {
    socket,
    message: (m) => {
      if (!ended) messages.push(m);
    },
    closed,
    tooLarge: () => {
      why = E.tooLarge;
      closed(1009, "too large");
    },
  };
}

/** The UTF-8 length of `text`. */
const utf8Length = (text) => new TextEncoder().encode(text).length;

/**
 * The browser's `k.http.StandardWebSocketClient` over `WebSocket`. A
 * browser can't send handshake headers (K4031 keeps them out of client
 * code) and doesn't say why a handshake failed, so a refusal is `offline`.
 * A cancelled `connect` never settles and closes what it opened.
 * @param {object} E the generated `WebSocketError` enum
 * @param {object} M the generated `WebSocketMessage` enum
 * @param {object} St the generated `WebSocketState` enum
 * @param {Function} [open] stands in for `new WebSocket(url, protocols)` in tests
 */
export function StandardWebSocketClient(E, M, St, open) {
  const make = open ?? ((url, p) => (p === null ? new WebSocket(url) : new WebSocket(url, p)));
  return {
    connect(c, r) {
      if (c?.aborted) return new Promise(() => {});
      return new Promise((resolve, reject) => {
        let ws;
        try {
          ws = make(String(r.url), r.subprotocol ?? null);
        } catch {
          reject(E.offline);
          return;
        }
        ws.binaryType = "arraybuffer";
        let s = null;
        let done = false;
        const give = (e) => {
          if (done) return;
          done = true;
          clearTimeout(timer);
          c?.removeEventListener?.("abort", cancel);
          if (e) reject(e);
        };
        const cancel = () => {
          give(null);
          ws.close();
        };
        const timer = setTimeout(() => {
          give(E.timeout);
          ws.close();
        }, r.timeout ?? 30000);
        c?.addEventListener?.("abort", cancel);
        const max = r.maxMessage ?? 1000000;
        ws.onopen = () => {
          if (done) return;
          give(null);
          s = openedSocket(
            { E, St },
            {
              send: (m) => ws.send(m._0),
              // A browser closes only with 1000 or 3000–4999.
              close: (code, reason) => {
                try {
                  ws.close(code, reason);
                } catch {
                  ws.close();
                }
              },
              buffered: () => ws.bufferedAmount,
            },
            ws.protocol,
            c,
          );
          resolve(s.socket);
        };
        ws.onmessage = (e) => {
          if (s === null) return;
          const text = typeof e.data === "string";
          if ((text ? utf8Length(e.data) : e.data.byteLength) > max) {
            s.tooLarge();
            ws.close();
          } else s.message(text ? M.text(e.data) : M.binary(new Uint8Array(e.data)));
        };
        ws.onclose = (e) => {
          if (s !== null) s.closed(e.code, e.reason);
          else give(E.offline);
        };
      });
    },
  };
}

// ------------------------------------------------------------- k test

/**
 * `k.test.TestWebSocketServer()`: a scripted endpoint. `received` holds what
 * the code under test sent; `send` and `close` act on every open
 * connection. `connect` is the client's side, used by
 * {@link testWebSocketClient}.
 */
export function TestWebSocketServer() {
  const open = new Set();
  const received = [];
  return {
    get received() {
      return received.slice();
    },
    get connections() {
      return open.size;
    },
    send(m) {
      for (const s of open) s.message(m);
    },
    close(code, reason) {
      for (const s of [...open]) {
        open.delete(s);
        s.closed(code ?? 1000, reason ?? "");
      }
    },
    /** Opens a connection to this endpoint for `T`'s client, owned by `c`. */
    connect(T, protocol, c) {
      let s = null;
      s = openedSocket(
        T,
        {
          send: (m) => {
            received.push(m);
          },
          close: (code, reason) => {
            open.delete(s);
            // The endpoint confirms at once.
            queueMicrotask(() => s.closed(code, reason));
          },
          buffered: () => 0,
        },
        protocol,
        c,
      );
      open.add(s);
      return s.socket;
    },
  };
}

/** A URL without its query and fragment, the key scripted endpoints are found by. */
const endpointOf = (url) => String(url).split(new RegExp("[?#]"))[0];

/**
 * `k.test.TestWebSocketClient`: connects only to scripted URLs, records
 * every request, and fails the test (a panic naming the URL) on any other.
 * @param {object} E the generated `WebSocketError` enum
 * @param {object} M the generated `WebSocketMessage` enum
 * @param {object} St the generated `WebSocketState` enum
 */
export function testWebSocketClient(E, M, St) {
  const T = { E, M, St };
  /** @type {Map<string, {server?: object, error?: unknown}>} */
  const scripted = new Map();
  const requests = [];
  return {
    get requests() {
      return requests.slice();
    },
    async connect(c, r) {
      requests.push(r);
      const a = scripted.get(endpointOf(r.url));
      if (!a) throw new KPanic(`TestWebSocketClient has no endpoint at ${endpointOf(r.url)}; script one with serve(url:with:)`);
      if (a.error !== undefined) throw a.error;
      return a.server.connect(T, r.subprotocol ?? null, c);
    },
    serve(url, server) {
      scripted.set(endpointOf(url), { server });
    },
    fail(url, error) {
      scripted.set(endpointOf(url), { error });
    },
  };
}
