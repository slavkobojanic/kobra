/**
 * @module k:runtime/share
 *
 * `k.device.BrowserShare` and `k.device.ShareButton` in the browser
 * (docs/design/45-web-platform.md, "Web Share"; docs/compiler/runtime.md
 * §12.6): the system share sheet through `navigator.share`, and a button
 * that shares where the browser can and copies the link where it can't.
 *
 * `ShareError`'s cases are `unitCase(0)` `unsupported`, `1` `cancelled`
 * and `2` `denied`; `ShareButtonFailure`'s `0` `denied` and `1`
 * `unsupported`.
 *
 * Shipped as its own client chunk, only to apps whose client shares
 * (codegen.md §16), and measured against its own size budget.
 */

import { claimRaw } from "./hydrate.js";
import { batch, onCleanup, renderEffect, untrack } from "./signals.js";
import { isPanic, unitCase } from "./values.js";

/** Whether the browser has a share sheet here (secure contexts only). */
const canShare = () => globalThis.isSecureContext === true && typeof globalThis.navigator?.share === "function";

/**
 * The `navigator.share` data of a `ShareContent` (`{url, text, title}`),
 * without its missing parts.
 * @param {{url?: string|null, text?: string|null, title?: string|null}} content
 */
export function shareData(content) {
  const data = {};
  for (const k of ["url", "text", "title"]) if (content[k] != null) data[k] = String(content[k]);
  return data;
}

/**
 * The browser's `k.device.BrowserShare`. `share` is async (its first
 * argument is the call's cancellation signal) and rejects with
 * `unsupported` where there is no sheet or `canShare` refuses the content
 * (or `share` throws a `TypeError`), `cancelled` on an `AbortError`, and
 * `denied` otherwise (`NotAllowedError`, a share already open).
 * @returns {{isSupported: boolean, share: Function}}
 */
export function BrowserShare() {
  return {
    get isSupported() {
      return canShare();
    },
    async share(_c, content) {
      const data = shareData(content);
      if (!canShare() || navigator.canShare?.(data) === false) throw unitCase(0);
      try {
        await navigator.share(data);
      } catch (e) {
        if (isPanic(e)) throw e;
        throw unitCase(e?.name === "AbortError" ? 1 : e?.name === "TypeError" || e instanceof TypeError ? 0 : 2);
      }
    },
  };
}

/** How long the button says "Link copied", in milliseconds. */
const SHARE_SHOWN = 2000;

/** `VisuallyHidden`'s styles (codegen.md §10), for the live region. */
const SHARE_HIDDEN = "position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0";

/**
 * The browser's `ShareButton`: with the container's `Share` where it is
 * supported, else copying the link (or the text) with its `Clipboard`.
 * It renders like `CopyButton` (`statusButton` in `clipboard.js`, a
 * chunk of its own, so the few lines are repeated here): a `span` with
 * the button, named `label`, and a polite live region.
 * @param {Record<string, Function>} props generated getters and `on$shared`, `on$copied`, `on$failed`
 * @param {{c: object}} ctx the render context, whose container has `Share`, `Clipboard` and `Clock`
 * @returns {Element}
 */
export function ShareButton(props, ctx) {
  const el = claimRaw("span") ?? document.createElement("span");
  while (el.firstChild) el.removeChild(el.firstChild);
  el.setAttribute("class", "k-status-button");
  const button = el.appendChild(document.createElement("button"));
  button.setAttribute("type", "button");
  const live = el.appendChild(document.createElement("span"));
  live.setAttribute("role", "status");
  live.setAttribute("style", SHARE_HIDDEN);
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
    timer = ctx.c.Clock.after(SHARE_SHOWN, () => show(null));
  };
  renderEffect(() => {
    const label = String(props.label());
    button.setAttribute("aria-label", label);
    if (shown === null) button.textContent = label;
  });
  /** Fires `failed` with `ShareButtonFailure` case `tag`; says so when nothing listens. */
  const fail = (tag) => {
    batch(() => props.on$failed?.(unitCase(tag)));
    return props.on$failed ? null : "Couldn't share";
  };
  const act = async () => {
    const content = untrack(props.content);
    const share = ctx.c.Share;
    if (share.isSupported) {
      try {
        await share.share(null, content);
        batch(() => props.on$shared?.());
        return null;
      } catch (e) {
        if (isPanic(e)) throw e;
        // Closing the sheet is an answer, not a failure; content the sheet
        // can't take (`unsupported`) falls through to the copy.
        if (e?.$ === 1) return null;
        if (e?.$ !== 0) return fail(0);
      }
    }
    const link = content.url != null;
    try {
      await ctx.c.Clipboard.write(null, String(link ? content.url : content.text ?? content.title ?? ""));
    } catch (e) {
      if (isPanic(e)) throw e;
      return fail(e?.$ === 0 ? 1 : 0);
    }
    batch(() => props.on$copied?.());
    return link ? "Link copied" : "Copied";
  };
  button.addEventListener("click", () => {
    act().then((message) => {
      if (message !== null) batch(() => show(message));
    });
  });
  onCleanup(() => timer?.cancel());
  return el;
}
