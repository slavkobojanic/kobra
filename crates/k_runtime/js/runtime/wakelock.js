/**
 * @module k:runtime/wakelock
 *
 * `k.device.KeepScreenAwake` in the browser
 * (docs/design/45-web-platform.md, "KeepScreenAwake";
 * docs/compiler/runtime.md §12.11): the screen's wake lock
 * (`navigator.wakeLock.request("screen")`) while the component is
 * mounted, asked for again when the page is shown, since the browser
 * releases it whenever the page is hidden.
 *
 * `WakeLockFailure`'s cases are `unitCase(0)` `unsupported` and `1`
 * `unavailable`.
 *
 * Shipped as its own client chunk, only to apps that use the component
 * (codegen.md §16), and measured against its own size budget.
 */

import { batch, onCleanup } from "./signals.js";
import { unitCase } from "./values.js";

/**
 * The browser's lock: a promise of its `WakeLockSentinel`, rejected with
 * the failure's case number (`0` without the API or a secure context,
 * `1` when the browser refuses).
 */
const screenLock = {
  request: () =>
    globalThis.isSecureContext === true && globalThis.navigator?.wakeLock
      ? navigator.wakeLock.request("screen").catch(() => Promise.reject(1))
      : Promise.reject(0),
};

/**
 * The browser's `KeepScreenAwake`: holds the container's `WakeLock` (the
 * test kit's `TestWakeLock`, whose `request` answers at once) or the
 * screen's while mounted; renders an empty comment. A refusal fires
 * `failed` with its case.
 * @param {Record<string, Function>} props `on$failed`
 * @param {{c: object}} ctx the render context
 * @returns {Comment}
 */
export function KeepScreenAwake(props, ctx) {
  const lock = ctx.c?.WakeLock ?? screenLock;
  let held = null;
  let asking = false;
  let mounted = true;
  const got = (s) => {
    asking = false;
    if (!mounted) return s.release();
    held = s;
    s.onrelease = () => held === s && (held = null);
  };
  const refused = (tag) => {
    asking = false;
    if (mounted) batch(() => props.on$failed?.(unitCase(tag)));
  };
  const ask = () => {
    if (held || asking || !mounted || globalThis.document?.visibilityState === "hidden") return;
    asking = true;
    try {
      const r = lock.request();
      if (r?.then) r.then(got, refused);
      else got(r);
    } catch (tag) {
      refused(tag);
    }
  };
  document.addEventListener?.("visibilitychange", ask);
  ask();
  onCleanup(() => {
    mounted = false;
    document.removeEventListener?.("visibilitychange", ask);
    held?.release();
    held = null;
  });
  return document.createComment("");
}
