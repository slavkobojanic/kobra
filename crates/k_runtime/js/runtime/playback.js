/**
 * @module k:runtime/playback
 *
 * The `video` and `audio` elements' binding (docs/design/46-media-and-rtc.md,
 * "Showing and playing media"; docs/compiler/runtime.md §12.14): lowering
 * calls `$k.mediaElement(el, source, options)` for each, and the bundler
 * routes the call to this chunk, as it does `observeElement`.
 *
 * `source` is a function giving the `MediaSource` (`{$: 0, _0: stream}`,
 * `{$: 1, _0: file}`, `{$: 2, _0: url}`); each option (`controls`,
 * `muted`, `monitor`, `loops`, `mirrored`, `poster`, `captions`,
 * `controller`) is a function too, read in an effect of its own.
 */

import { onCleanup, renderEffect } from "./signals.js";

/** The `Captions.track` element a `video` holds, if any. */
const captionTrack = (el) => [...el.childNodes].find((n) => n.tagName === "TRACK") ?? null;

/** Sets or removes the boolean attribute `name`, with its property. */
function flag(el, name, prop, on) {
  el[prop] = on;
  if (on) el.setAttribute(name, "");
  else el.removeAttribute(name);
}

/**
 * Binds `el` to what it plays: `srcObject` for a stream (inline,
 * starting by itself, and muted when it is local unless `monitor`), a
 * managed object URL for a file (revoked when the source changes or the
 * element unmounts), `src` for a URL. The browser's controls show for a
 * file or URL unless `controls` says otherwise. A stream or file whose
 * sound the browser won't start without a user's action fires
 * `playbackblocked` (the `playbackBlocked` event).
 * @param {HTMLMediaElement} el
 * @param {() => {$: number, _0: unknown}} source
 * @param {Record<string, () => unknown>} o
 */
export function mediaElement(el, source, o) {
  let url = null;
  const revoke = () => {
    if (url !== null) globalThis.URL?.revokeObjectURL?.(url);
    url = null;
  };
  el.setAttribute("playsinline", "");
  renderEffect(() => {
    const s = source();
    revoke();
    el.$kSource = s;
    const stream = s.$ === 0 ? s._0 : null;
    if (stream !== null) {
      el.removeAttribute("src");
      el.srcObject = stream.$s ?? null;
      el.autoplay = true;
    } else {
      el.srcObject = null;
      if (s.$ === 1) {
        const f = s._0;
        url = typeof Blob === "function" ? globalThis.URL?.createObjectURL?.(new Blob([f.bytes], { type: f.mediaType })) ?? null : null;
      }
      const src = s.$ === 1 ? url : String(s._0);
      if (src === null) el.removeAttribute("src");
      else el.setAttribute("src", src);
    }
    flag(el, "controls", "controls", o.controls ? o.controls() === true : stream === null);
    // Hearing your own microphone through the speakers is feedback.
    const quiet = (o.muted?.() ?? false) === true || (stream?.isLocal === true && (o.monitor?.() ?? false) !== true);
    flag(el, "muted", "muted", quiet);
    if (stream !== null && stream.$s) {
      el.play?.()?.catch?.((e) => e?.name === "NotAllowedError" && el.dispatchEvent(new Event("playbackblocked")));
    }
  });
  if (o.loops) renderEffect(() => flag(el, "loop", "loop", o.loops() === true));
  if (o.mirrored) renderEffect(() => (el.style.transform = o.mirrored() === true ? "scaleX(-1)" : ""));
  if (o.poster) renderEffect(() => el.setAttribute("poster", String(o.poster())));
  if (o.captions) {
    renderEffect(() => {
      const c = o.captions();
      captionTrack(el)?.remove();
      if (c.$ !== 0) return;
      const t = document.createElement("track");
      for (const [k, v] of [["kind", "captions"], ["src", String(c.url)], ["srclang", c.language], ["label", c.label], ["default", ""]]) t.setAttribute(k, v);
      el.appendChild(t);
    });
  }
  if (o.controller) {
    renderEffect(() => {
      const c = o.controller();
      c.$attach(el);
      onCleanup(() => c.$detach(el));
    });
  }
  onCleanup(revoke);
}
