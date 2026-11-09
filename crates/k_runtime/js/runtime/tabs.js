/**
 * @module k:runtime/tabs
 *
 * `k.device.BrowserTabMessaging` in the browser
 * (docs/design/45-web-platform.md, "Messages between tabs";
 * docs/compiler/runtime.md §12.10): one `BroadcastChannel` per channel
 * name and tab, named `namespace:name` as storage keys are, which the
 * standard delivers to the app's other tabs and never to the one that
 * posted. A message is `{b, m}`: the build that posted it and the JSON
 * text of its codec's form; a tab drops another build's messages.
 *
 * Shipped as its own client chunk, only to apps whose client injects
 * `TabMessaging` (codegen.md §16), and measured against its own size
 * budget.
 */

import { PushStream, stringify } from "./std.js";
import { batch, onCleanup } from "./signals.js";
import { isPanic } from "./values.js";

/**
 * The browser's `k.device.BrowserTabMessaging` for the app's storage
 * `namespace` and `build` (the URL of the client entry, whose name
 * changes with every build). Every member takes the message type's codec
 * first. Without `BroadcastChannel` nothing is posted or heard.
 * @param {string} namespace
 * @param {string} build
 * @returns {object}
 */
export function BrowserTabMessaging(namespace, build) {
  /** Per channel name: the listeners of this tab and the channel object. */
  const open = new Map();
  const channel = (c) => {
    let e = open.get(c.name);
    if (!e) {
      e = { subs: new Set(), ch: null };
      if (typeof globalThis.BroadcastChannel === "function") {
        e.ch = new BroadcastChannel(namespace ? `${namespace}:${c.name}` : c.name);
        e.ch.onmessage = ({ data }) => {
          if (data?.b === build) for (const f of [...e.subs]) f(data.m);
        };
      }
      open.set(c.name, e);
    }
    return e;
  };
  /** Calls `f` with each decoded message on `c`; returns the unsubscribe, `true` the first time. */
  const sub = (codec, c, f) => {
    const { subs } = channel(c);
    const g = (text) => {
      let v;
      try {
        v = codec.decode(JSON.parse(text), "");
      } catch (e) {
        if (isPanic(e)) throw e;
        return;
      }
      batch(() => f(v));
    };
    subs.add(g);
    return () => subs.delete(g);
  };
  return {
    post(codec, c, message) {
      channel(c).ch?.postMessage({ b: build, m: stringify(codec.encode(message)) });
    },
    listen(codec, c, handler) {
      return { cancel: sub(codec, c, handler) };
    },
    messages(codec, c) {
      const stream = new PushStream();
      const off = sub(codec, c, (v) => stream.push(v));
      stream.close = () => off() && stream.push(null);
      onCleanup(stream.close);
      return stream;
    },
  };
}
