/**
 * @module k:runtime/observe
 *
 * The `appeared`, `disappeared` and `resized` events every element has
 * (docs/design/45-web-platform.md, "Element events";
 * docs/compiler/runtime.md §12.9), on one shared `IntersectionObserver` per
 * distinct `appearThreshold` and `appearMargin`, and one `ResizeObserver`
 * per page.
 *
 * `appeared` fires each time at least the threshold of the element becomes
 * visible in the viewport grown by the margin, and `disappeared` each time
 * it stops being visible after it appeared; the observer's first report of
 * an element that isn't visible fires nothing. `resized` fires with the
 * content box's size (an `ElementSize` of pixel `Length`s) once after
 * mount and on each change, at most once per frame, as the browser reports
 * it.
 *
 * The element is observed from its creation; the browser reports it only
 * once it is in the document, after mount. Observing stops when the
 * component that wrote the handler is disposed. A
 * handler runs as an `on` handler does: inside `batch`, never after its
 * owner was disposed, dropping its `CANCELLED`. Where the browser lacks an
 * observer (the server never runs this), nothing fires.
 *
 * A runtime feature with a chunk of its own, shipped only to clients that
 * use one of the events (codegen.md §16).
 */

import { CANCELLED } from "./values.js";
import { batch, getOwner, onCleanup } from "./signals.js";

/**
 * The shared observers: an `IntersectionObserver` per `threshold margin`
 * key, and `"resize"` for the `ResizeObserver`. Each holds its observer
 * (`o`, made with the constructor `C`) and its elements (`els`: element →
 * `{seen, appeared, disappeared, resized}`, whether the element is visible
 * and the handler sets).
 * @type {Map<string, {C: Function, o: object, els: Map<Element, object>}>}
 */
const observers = new Map();

/**
 * The shared observer for `key`, created as `new C(callback, options)` on
 * first use (and again when `C` was replaced, as each `k test` test does).
 * The callback runs each reported element's handlers through `fire`.
 */
function shared(key, C, options, fire) {
  let s = observers.get(key);
  if (s?.C === C) return s;
  const els = new Map();
  const o = new C((entries) => {
    for (const e of entries) {
      const rec = els.get(e.target);
      if (rec) fire(rec, e);
    }
  }, options);
  s = { C, o, els };
  observers.set(key, s);
  return s;
}

/**
 * Observes `el` for `kind` (`"appeared"`, `"disappeared"` or
 * `"resized"`), running `handler` (with the `ElementSize` for
 * `resized`). `threshold` is the `appearThreshold` (`Percent`, 0–100) and
 * `margin` the `appearMargin` (a CSS length), `null` for the defaults.
 * @param {Element} el
 * @param {string} kind
 * @param {(payload?: object) => unknown} handler
 * @param {number|null} [threshold]
 * @param {string|null} [margin]
 */
export function observeElement(el, kind, handler, threshold, margin) {
  const owner = getOwner();
  const run = (payload) => {
    if (owner !== null && owner.disposed) return;
    let r;
    try {
      r = batch(() => handler(payload));
    } catch (x) {
      if (x === CANCELLED) return;
      throw x;
    }
    r?.then?.(undefined, (x) => {
      if (x !== CANCELLED) throw x;
    });
  };
  const g = globalThis;
  const resize = kind === "resized";
  const Observer = resize ? g.ResizeObserver : g.IntersectionObserver;
  if (typeof Observer !== "function") return;
  const t = (threshold ?? 0) / 100;
  const m = margin ?? "0px";
  const s = resize
    ? shared("resize", Observer, undefined, (rec, e) => {
      const box = e.contentRect;
      const size = { width: `${box.width}px`, height: `${box.height}px` };
      for (const h of rec.resized) h(size);
    })
    : shared(`${t} ${m}`, Observer, { threshold: t, rootMargin: m }, (rec, e) => {
      const visible = e.isIntersecting === true && e.intersectionRatio >= t;
      if (visible === rec.seen) return;
      rec.seen = visible;
      for (const h of visible ? rec.appeared : rec.disappeared) h();
    });
  let rec = s.els.get(el);
  if (!rec) {
    rec = { seen: false, appeared: new Set(), disappeared: new Set(), resized: new Set() };
    s.els.set(el, rec);
    s.o.observe(el);
  }
  rec[kind].add(run);
  onCleanup(() => {
    rec[kind].delete(run);
    if (rec.appeared.size + rec.disappeared.size + rec.resized.size === 0) {
      s.els.delete(el);
      s.o.unobserve(el);
    }
  });
}
