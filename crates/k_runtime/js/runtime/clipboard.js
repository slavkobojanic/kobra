/**
 * @module k:runtime/clipboard
 *
 * `k.device.BrowserClipboard` and `k.device.CopyButton` in the browser
 * (docs/design/45-web-platform.md, "Clipboard"; docs/compiler/runtime.md
 * §12.6): plain text through `navigator.clipboard`, and a button that
 * copies, says "Copied" for two seconds and announces it.
 *
 * The errors are the stdlib enums' cases, which need no generated code:
 * `unsupported` is `unitCase(0)` and `denied` `unitCase(1)` in both
 * `ClipboardWriteError` and `ClipboardReadError`.
 *
 * Shipped as its own client chunk, only to apps whose client copies
 * (codegen.md §16), and measured against its own size budget.
 */

import { claimRaw } from "./hydrate.js";
import { batch, onCleanup, renderEffect, untrack } from "./signals.js";
import { isPanic, unitCase } from "./values.js";

/** `VisuallyHidden`'s styles (codegen.md §10), for the live region. */
export const STATUS_HIDDEN = "position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0";

/** How long a button says "Copied", in milliseconds. */
export const STATUS_SHOWN = 2000;

/** The browser's clipboard, or `undefined` outside a secure context. */
const clipboardApi = () => (globalThis.isSecureContext === true ? globalThis.navigator?.clipboard : undefined);

/** A refusal as `denied`, a panic as itself. */
const copyRefused = (e) => {
  throw isPanic(e) ? e : unitCase(1);
};

/**
 * The browser's `k.device.BrowserClipboard`. `isSupported` is constant for
 * the page's life; `write` and `readText` are async, so their first
 * argument is the call's cancellation signal. Browsers report a refused
 * permission and a missing user activation alike (`NotAllowedError`), so
 * both are `denied`.
 * @returns {{isSupported: boolean, write: Function, readText: Function}}
 */
export function BrowserClipboard() {
  return {
    get isSupported() {
      return typeof clipboardApi()?.writeText === "function";
    },
    async write(_c, text) {
      const a = clipboardApi();
      if (typeof a?.writeText !== "function") throw unitCase(0);
      await a.writeText(String(text)).catch(copyRefused);
    },
    /** The clipboard's text; `null` when it holds none (Gecko's `NotFoundError`, or `""`). */
    async readText(_c) {
      const a = clipboardApi();
      if (typeof a?.readText !== "function") throw unitCase(0);
      const text = await a.readText().catch((e) => (e?.name === "NotFoundError" ? "" : copyRefused(e)));
      return text === "" ? null : String(text);
    },
  };
}

/**
 * A button that shows `label`, and after `act` resolves says what it
 * reports for two seconds on `clock`, in its text and in a polite live
 * region: the shape `CopyButton` and `ShareButton` share. While hydrating
 * it takes the server's `span` (its content is redrawn the same). The
 * button keeps its width while it shows the message.
 * @param {Record<string, Function>} props generated getters (`label`)
 * @param {() => object} clock the injected `Clock`
 * @param {() => Promise<string|null>} act runs on a click; resolves to the message, or `null` for none
 * @returns {Element}
 */
export function statusButton(props, clock, act) {
  const el = claimRaw("span") ?? document.createElement("span");
  while (el.firstChild) el.removeChild(el.firstChild);
  el.setAttribute("class", "k-status-button");
  const button = el.appendChild(document.createElement("button"));
  button.setAttribute("type", "button");
  const live = el.appendChild(document.createElement("span"));
  live.setAttribute("role", "status");
  live.setAttribute("style", STATUS_HIDDEN);
  let shown = null;
  let timer = null;
  const show = (message) => {
    timer?.cancel();
    timer = null;
    shown = message;
    live.textContent = message ?? "";
    if (message === null) {
      button.style.removeProperty("min-width");
      button.textContent = String(untrack(props.label));
      return;
    }
    const width = button.getBoundingClientRect?.().width;
    if (width) button.style.setProperty("min-width", `${width}px`);
    button.textContent = message;
    timer = clock().after(STATUS_SHOWN, () => show(null));
  };
  renderEffect(() => {
    const label = String(props.label());
    button.setAttribute("aria-label", label);
    if (shown === null) button.textContent = label;
  });
  button.addEventListener("click", () => {
    act().then((message) => {
      if (message !== null) batch(() => show(message));
    });
  });
  onCleanup(() => timer?.cancel());
  return el;
}

/**
 * The browser's `CopyButton`: copies `text` (read at the click) with the
 * container's `Clipboard`, then fires `copied`, or `failed` with the
 * error, showing "Couldn't copy" when nothing listens.
 * @param {Record<string, Function>} props generated getters and `on$copied`, `on$failed`
 * @param {{c: object}} ctx the render context, whose container has `Clipboard` and `Clock`
 * @returns {Element}
 */
export function CopyButton(props, ctx) {
  return statusButton(props, () => ctx.c.Clock, async () => {
    try {
      await ctx.c.Clipboard.write(null, String(untrack(props.text)));
    } catch (e) {
      if (isPanic(e)) throw e;
      batch(() => props.on$failed?.(e));
      return props.on$failed ? null : "Couldn't copy";
    }
    batch(() => props.on$copied?.());
    return "Copied";
  });
}
