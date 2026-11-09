/**
 * @module k:runtime/geolocation
 *
 * `k.device.BrowserGeolocation` in the browser
 * (docs/design/45-web-platform.md, "Geolocation"; docs/compiler/runtime.md
 * §12.7): positions through `navigator.geolocation`, the permission from
 * the Permissions API, kept in a signal so views that read it follow the
 * site's settings.
 *
 * The enums are the stdlib's cases: `PermissionState` `unitCase(0)`
 * `prompt`, `1` `granted`, `2` `denied`; `LocationError` `0` `unsupported`,
 * `1` `notAsked`, `2` `denied`, `3` `unavailable`, `4` `timeout`;
 * `LocationAccuracy` `0` `coarse`, `1` `precise`.
 *
 * Shipped as its own client chunk, only to apps whose client locates
 * (codegen.md §16), and measured against its own size budget.
 */

import { Instant, PushStream } from "./std.js";
import { onCleanup, signal } from "./signals.js";
import { unitCase } from "./values.js";

/** `PermissionState` by the Permissions API's state name. */
const STATES = { prompt: 0, granted: 1, denied: 2 };

/**
 * A `Position` of a browser `GeolocationPosition`: a heading the device
 * can't tell (`NaN` while still, or `null`) and other unknowns are `nil`.
 * @param {GeolocationPosition} p
 * @returns {object}
 */
export function position(p) {
  const c = p.coords;
  const known = (v) => (typeof v === "number" && !Number.isNaN(v) ? v : null);
  return {
    latitude: c.latitude,
    longitude: c.longitude,
    accuracyMeters: c.accuracy,
    altitudeMeters: known(c.altitude),
    headingDegrees: known(c.heading),
    speedMetersPerSecond: known(c.speed),
    measuredAt: new Instant(p.timestamp),
  };
}

/**
 * The browser's `k.device.BrowserGeolocation`. `isSupported` is constant
 * for the page's life; `permission` is reactive, `.prompt` until the
 * Permissions API answers, and follows its `change` event (re-read when
 * the page becomes visible, for engines that don't fire it). Where there
 * is no Permissions API, or it says `prompt` after the user allowed
 * (Gecko without "Remember"), the answer to `requestAccess` is
 * remembered for the page's life.
 *
 * `requestAccess` and `currentPosition` are async (their first argument
 * is the call's cancellation signal); `currentPosition` reads the
 * permission first, so it never prompts. `watch` throws synchronously,
 * reading the permission reactively, so an effect that called it before
 * the Permissions API answered runs again; its stream ends when the
 * permission stops being granted or its owner is disposed, clearing the
 * browser's watch.
 * @returns {object}
 */
export function BrowserGeolocation() {
  const api = () => (globalThis.isSecureContext === true ? globalThis.navigator?.geolocation : undefined);
  const state = signal(unitCase(0));
  const open = new Set();
  const set = (s) => {
    state.set(unitCase(s));
    if (s !== 1) for (const close of [...open]) close();
  };
  let status = null;
  // Gecko keeps a grant without "Remember" for the page's life while the
  // Permissions API still says `prompt`: a grant seen here counts.
  let granted = false;
  const fromStatus = () => {
    const s = STATES[status.state] ?? 0;
    set(s === 0 && granted ? 1 : s);
  };
  /** Reads the permission (once a status is open, it is kept current). */
  const query = async () => {
    try {
      status ??= await navigator.permissions.query({ name: "geolocation" });
      status.onchange ??= fromStatus;
      fromStatus();
    } catch {
      // No Permissions API, or no geolocation in it: keep what is known.
    }
    return state.peek().$;
  };
  if (api()) {
    query();
    globalThis.document?.addEventListener?.("visibilitychange", () => document.visibilityState === "visible" && query());
  }
  /** One position from the browser, or the `GeolocationPositionError`'s code. */
  const once = (options) =>
    new Promise((resolve, reject) => api().getCurrentPosition(resolve, (e) => reject(e?.code ?? 2), options));
  return {
    get isSupported() {
      return typeof api()?.getCurrentPosition === "function";
    },
    get permission() {
      return state.get();
    },
    async requestAccess(_c) {
      if (!api()) return state.peek();
      const now = await query();
      if (now !== 0) return unitCase(now);
      // The browser has no prompt-only call: asking for a position prompts.
      const code = await once({ maximumAge: Infinity }).then(() => 0, (c) => c);
      granted = code !== 1;
      set(code === 1 ? 2 : (await query()) === 2 ? 2 : 1);
      return state.peek();
    },
    async currentPosition(_c, accuracy, maximumAge, timeout) {
      if (!api()) throw unitCase(0);
      const now = await query();
      if (now !== 1) throw unitCase(now === 0 ? 1 : 2);
      try {
        return position(await once({ enableHighAccuracy: accuracy?.$ === 1, maximumAge, timeout }));
      } catch (code) {
        if (code === 1) set(2), (granted = false);
        throw unitCase(code === 1 ? 2 : code === 3 ? 4 : 3);
      }
    },
    watch(accuracy) {
      const a = api();
      if (!a) throw unitCase(0);
      // Read reactively: an effect that watched before the permission was
      // known runs again once it is.
      const now = state.get().$;
      if (now !== 1) throw unitCase(now === 0 ? 1 : 2);
      const stream = new PushStream();
      const id = a.watchPosition(
        (p) => stream.push(position(p)),
        // A momentary loss of fix yields nothing; a refusal ends the stream.
        (e) => e?.code === 1 && ((granted = false), set(2)),
        { enableHighAccuracy: accuracy?.$ === 1 },
      );
      const close = () => {
        if (!open.delete(close)) return;
        a.clearWatch(id);
        stream.push(null);
      };
      open.add(close);
      stream.close = close;
      onCleanup(close);
      return stream;
    },
  };
}
