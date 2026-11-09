/**
 * @module k:runtime/streams
 *
 * Server-streaming rpcs in the browser (docs/design/29-server-streaming.md;
 * docs/compiler/runtime.md §8; the wire is server.md §6): the request is
 * one Connect envelope, and the response a sequence of envelopes read as
 * they arrive. Generated stubs of a streaming rpc call
 * `rpc(path, message, $c, streamed(each))`.
 *
 * Shipped as its own client chunk, only to apps with a streaming rpc
 * (codegen.md §16), and measured against its own size budget.
 */

import { takeMirror } from "./rpc.js";
import { PushStream } from "./std.js";

/**
 * The `rpc` options of a server-streaming call whose elements `each`
 * decodes (a struct element is the message itself, anything else
 * `{"value": …}`): `stream.body(message)` is the request envelope, and
 * `stream.read(response, controller, signal)` the open stream.
 * @param {(j: unknown) => unknown} each
 * @returns {{stream: {body: (message: unknown) => Uint8Array, read: Function}}}
 */
export function streamed(each) {
  return {
    stream: {
      body: (message) => {
        const json = new TextEncoder().encode(JSON.stringify(message ?? {}));
        const n = json.length;
        const out = new Uint8Array(5 + n);
        out.set([0, n >>> 24, (n >>> 16) & 255, (n >>> 8) & 255, n & 255]);
        out.set(json, 5);
        return out;
      },
      read: (response, controller, signal) => readStream(response, each, controller, signal),
    },
  };
}

/**
 * The elements of a streamed response body (Connect envelopes), decoded
 * with `each`, as a `PushStream` that yields each as its envelope arrives
 * and ends at the end-of-stream envelope (an error in it is no error; an
 * `x-k-session` in its metadata, sent when the stream's session ended, is
 * taken as a response header's is),
 * when the connection drops, or when an element `each` refuses arrives.
 * Closing it (leaving a `for await` loop) or aborting `signal` (the
 * caller was cancelled) aborts the request and cancels the body.
 * @param {Response} response
 * @param {(j: unknown) => unknown} each
 * @param {AbortController|null} controller
 * @param {AbortSignal|null} signal
 * @returns {PushStream}
 */
function readStream(response, each, controller, signal) {
  const stream = new PushStream();
  const reader = response.body?.getReader?.();
  stream.close = () => {
    controller?.abort();
    reader?.cancel?.()?.catch?.(() => {});
  };
  signal?.addEventListener("abort", stream.close);
  (async () => {
    const text = new TextDecoder();
    let buf = new Uint8Array(0);
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) return;
        const joined = new Uint8Array(buf.length + value.length);
        joined.set(buf);
        joined.set(value, buf.length);
        buf = joined;
        for (let n; buf.length >= 5 && buf.length >= 5 + (n = ((buf[1] << 24) | (buf[2] << 16) | (buf[3] << 8) | buf[4]) >>> 0);) {
          // The end: one that ended with its session carries the guest
          // mirror, taken before the stream ends, so a guarded page is left
          // before its code can open the stream again.
          if (buf[0] & 2) return takeMirror(JSON.parse(text.decode(buf.subarray(5, 5 + n))).metadata?.["x-k-session"]?.[0]);
          stream.push(each(JSON.parse(text.decode(buf.subarray(5, 5 + n)))));
          buf = buf.slice(5 + n);
        }
      }
    } catch {
      // A dropped connection, an abort or an undecodable element ends the
      // stream like its end: ending is not an error.
    } finally {
      stream.push(null);
    }
  })();
  return stream;
}
