/**
 * @module k:runtime/environment
 *
 * The browser bodies of `k.device`'s environment services
 * (docs/design/45-web-platform.md, "Page visibility", "Connectivity",
 * "Preferences", "Viewport"; docs/compiler/runtime.md §12.8): each member
 * is a signal following the browser's event, so a view or an effect that
 * reads one updates with the page.
 *
 * While the first page hydrates every member answers what the server
 * rendered with (visible, online, the preferences in `k-data`'s `p`, an
 * unknown viewport), and takes the browser's value once hydration has
 * finished, as an expected update rather than a mismatch.
 *
 * `PointerKind` is the stdlib's enum: `unitCase(0)` `fine`, `1` `coarse`,
 * `2` `none`.
 *
 * Shipped as its own client chunk, only to apps whose client injects one of
 * the services (codegen.md §16), and measured against its own size budget.
 */

import { isHydrating } from "./hydrate.js";
import { afterHydration } from "./services.js";
import { signal } from "./signals.js";
import { equals, unitCase } from "./values.js";

/** Listens to `type` on `target`, when there is one. */
const listenTo = (target, type, f) => target?.addEventListener?.(type, f);

/**
 * A signal of `read()` that holds `rendered`, the server's value, until
 * hydration has finished; `s.refresh()` re-reads it, keeping an equal
 * value (a viewport size), so its readers run only on a change.
 * @param {unknown} rendered
 * @param {() => unknown} read
 */
function liveValue(rendered, read) {
  const s = signal(isHydrating() ? rendered : read());
  s.refresh = () => {
    const v = read();
    if (!equals(s.peek(), v)) s.set(v);
  };
  afterHydration(s.refresh);
  return s;
}

/**
 * The browser's `k.device.PageVisibility`: `isVisible` follows
 * `document.visibilityState`, re-read on `visibilitychange` and when the
 * page is restored from the back-forward cache (`pageshow`).
 * @returns {{isVisible: boolean}}
 */
export function PageVisibility() {
  const d = globalThis.document;
  const s = liveValue(true, () => d?.visibilityState !== "hidden");
  listenTo(d, "visibilitychange", s.refresh);
  listenTo(globalThis, "pageshow", s.refresh);
  return {
    get isVisible() {
      return s.get();
    },
  };
}

/**
 * The browser's `k.device.Connectivity`: `isOnline` follows
 * `navigator.onLine` and its `online` and `offline` events. The app's own
 * requests (rpcs, whose paths start with `/`) are the probe: one that fails
 * for want of a network (a rejected `fetch` that wasn't aborted) sets
 * `isOnline` to `false`, and one that gets an answer sets it to `true`,
 * so the indicator and the rpc's `.offline` agree, unless `navigator.onLine`
 * is `false`: that is certain, and a response that was already on its way
 * when the network went (an rpc, a stream's head) doesn't overrule it. They
 * are observed by
 * wrapping `fetch` once the service exists, which keeps the runtime chunk
 * out of it. The channels' socket says the same with the window events
 * `k-online` (it opened) and `k-offline` (a reconnect failed).
 * @returns {{isOnline: boolean}}
 */
export function Connectivity() {
  const g = globalThis;
  const onLine = () => g.navigator?.onLine !== false;
  const s = liveValue(true, onLine);
  // Evidence of a network: online, unless the browser knows it has none.
  const answered = () => s.set(onLine());
  listenTo(g, "online", s.refresh);
  listenTo(g, "offline", s.refresh);
  // The channels' socket is a probe too (channels.js): it opened, or a reconnect failed.
  listenTo(g, "k-online", answered);
  listenTo(g, "k-offline", () => s.set(false));
  const f = g.fetch;
  if (typeof f === "function") {
    g.fetch = (url, init) => {
      const p = f(url, init);
      return typeof url === "string" && url[0] === "/"
        ? p.then(
          (r) => (answered(), r),
          (e) => {
            if (e?.name !== "AbortError") s.set(false);
            throw e;
          },
        )
        : p;
    };
  }
  return {
    get isOnline() {
      return s.get();
    },
  };
}

/**
 * The browser's `k.device.Preferences`: each member follows its
 * `matchMedia` query's `change` event. While hydrating, dark mode and
 * reduced motion are what the server rendered with (`data.p`), and the
 * others `false`.
 * @param {{p?: boolean[]}|null} data the `k-data` payload
 */
export function Preferences(data) {
  const rendered = data?.p ?? [];
  const query = (q, i) => {
    const m = globalThis.matchMedia?.(q);
    const s = liveValue(rendered[i] === true, () => m?.matches === true);
    listenTo(m, "change", s.refresh);
    return s;
  };
  const dark = query("(prefers-color-scheme: dark)", 0);
  const motion = query("(prefers-reduced-motion: reduce)", 1);
  const contrast = query("(prefers-contrast: more)", 2);
  const forced = query("(forced-colors: active)", 3);
  return {
    get prefersDark() {
      return dark.get();
    },
    get prefersReducedMotion() {
      return motion.get();
    },
    get prefersMoreContrast() {
      return contrast.get();
    },
    get forcedColors() {
      return forced.get();
    },
  };
}

/** The `pointer:` media values, by `PointerKind` case. */
const POINTERS = ["fine", "coarse", "none"];

/**
 * The browser's `k.device.Viewport`: `size` is the layout viewport
 * (`innerWidth`, `innerHeight`, as `Length`s in pixels), `pointer` the
 * primary pointer and `canHover` whether it hovers, all `null` while
 * hydrating, as the server rendered them. A `resize`, or a change of the
 * pointer's media queries, re-reads them at most once per animation frame.
 * @returns {{size: object|null, pointer: object|null, canHover: boolean|null}}
 */
export function Viewport() {
  const g = globalThis;
  const media = (q) => g.matchMedia?.(q);
  const size = liveValue(null, () => (g.innerWidth === undefined ? null : { width: `${g.innerWidth}px`, height: `${g.innerHeight}px` }));
  const pointer = liveValue(null, () => {
    const i = POINTERS.findIndex((p) => media(`(pointer: ${p})`)?.matches);
    return i < 0 ? null : unitCase(i);
  });
  const hover = liveValue(null, () => media("(hover: hover)")?.matches === true);
  let queued = false;
  const update = () => {
    if (queued) return;
    queued = true;
    (g.requestAnimationFrame ?? setTimeout)(() => {
      queued = false;
      size.refresh();
      pointer.refresh();
      hover.refresh();
    });
  };
  listenTo(g, "resize", update);
  for (const q of ["(pointer: fine)", "(pointer: coarse)", "(hover: hover)"]) listenTo(media(q), "change", update);
  return {
    get size() {
      return size.get();
    },
    get pointer() {
      return pointer.get();
    },
    get canHover() {
      return hover.get();
    },
  };
}
