/**
 * @module k:runtime/refs
 *
 * `k.ui.ElementRef` in the browser (docs/design/53-element-references.md;
 * docs/compiler/runtime.md §12.19): a reference names the one element its
 * `.ref(…)` attaches (`$attach`, by the element binding), until the
 * element's owner is disposed. `focus`, `blur` and `scrollIntoView` run
 * once the view has caught up with the state changed before them: each is
 * a one-shot effect, which runs after the render effects of the flush that
 * ends the handler's batch (or of the next one, after an `await`). An
 * operation finds the element attached when it runs, and does nothing
 * while none is, or while it isn't in the document.
 *
 * Shipped as its own client chunk, only to clients that make a reference
 * (codegen.md §16), and measured against its own size budget.
 */

import { effect, onCleanup, root, untrack } from "./signals.js";

/** `ScrollAlignment` by case index. */
const ALIGN = ["start", "center", "end", "nearest"];

/**
 * `ElementRef()`: a reference naming no element yet.
 * @returns {object}
 */
export function ElementRef() {
  let el = null;
  /** Runs `f` on the element once the view has caught up. */
  const later = (f) =>
    root((dispose) =>
      effect(() => {
        dispose();
        let n = el;
        while (n?.parentNode) n = n.parentNode;
        if (n?.nodeType === 9) untrack(() => f(el));
      }),
    );
  return {
    focus(selecting) {
      later((e) => {
        // A heading or a section takes the focus from code, not from Tab.
        if (e.tabIndex < 0 && !e.hasAttribute("tabindex")) e.setAttribute("tabindex", "-1");
        e.focus();
        if (selecting) e.select?.();
      });
    },
    blur() {
      later((e) => e.blur());
    },
    scrollIntoView(align, animated) {
      later((e) =>
        e.scrollIntoView({
          block: ALIGN[align?.$ ?? 3],
          behavior: animated && !globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ? "smooth" : "auto",
        }),
      );
    },
    $attach(e) {
      el = e;
      onCleanup(() => {
        if (el === e) el = null;
      });
    },
  };
}
