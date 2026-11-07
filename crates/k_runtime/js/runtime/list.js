/**
 * @module k:runtime/list
 *
 * Keyed lists (`for … key`, docs/compiler/runtime.md §5.3): one row per
 * key between two comment markers, moved with a longest-increasing-
 * subsequence diff.
 */

import { claimRange, isHydrating } from "./hydrate.js";
import { place } from "./dom.js";
import { getOwner, renderEffect, runWithOwner, scope, signal, untrack } from "./signals.js";

/** Longest increasing subsequence of `a` (ignoring -1): the positions kept. */
function lis(a) {
  const tails = [];
  const prev = new Array(a.length).fill(-1);
  for (let i = 0; i < a.length; i++) {
    if (a[i] < 0) continue;
    let lo = 0;
    let hi = tails.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (a[tails[mid]] < a[i]) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0) prev[i] = tails[lo - 1];
    tails[lo] = i;
  }
  const keep = new Set();
  for (let i = tails.length ? tails[tails.length - 1] : -1; i >= 0; i = prev[i]) keep.add(i);
  return keep;
}

/**
 * The nodes of a row now: from its first node to its last, siblings in
 * between included, so content a block of the row inserted after the row
 * was created moves and leaves with it.
 * @param {{first: Node|null, last: Node|null}} r
 * @returns {Node[]}
 */
function span(r) {
  const out = [];
  for (let n = r.first; n; n = n === r.last ? null : n.nextSibling) out.push(n);
  return out;
}

/**
 * Renders one row per item between the markers, reusing rows by key and
 * moving them with a longest-increasing-subsequence diff (`for … key`):
 * the fewest nodes move, removed rows are disposed, and a row whose item
 * changed under the same key updates in place through its item signal.
 * @template T, K
 * @param {Comment} start
 * @param {Comment} end
 * @param {() => T[]} items
 * @param {(item: T) => K} key
 * @param {(item: () => T, index: () => number) => Node} row
 */
export function each(start, end, items, key, row) {
  const parent = getOwner();
  /** @type {Array<{key: K, item: any, index: any, first: Node|null, last: Node|null, dispose: () => void}>} */
  let rows = [];
  let first = true;
  const create = (value, i) => {
    const item = signal(value);
    const index = signal(i);
    const [node, dispose] = runWithOwner(parent, () => scope((d) => [row(() => item.get(), () => index.get()), d]));
    const frag = node != null && node.nodeType === 11;
    return { key: key(value), item, index, first: frag ? node.firstChild : node, last: frag ? node.lastChild : node, dispose };
  };
  renderEffect(() => {
    const list = items();
    untrack(() => {
      if (first && isHydrating()) {
        first = false;
        claimRange(start, end, () => {
          rows = list.map(create);
        });
        for (const r of rows) for (const n of span(r)) place(n, end);
        return;
      }
      first = false;
      const host = end.parentNode;
      const byKey = new Map();
      rows.forEach((r, i) => byKey.set(r.key, i));
      const next = [];
      const oldIndex = [];
      const used = new Set();
      for (let i = 0; i < list.length; i++) {
        const k = key(list[i]);
        const j = byKey.get(k);
        if (j !== undefined && !used.has(j)) {
          used.add(j);
          const r = rows[j];
          r.item.set(list[i]);
          r.index.set(i);
          next.push(r);
          oldIndex.push(j);
        } else {
          next.push(null);
          oldIndex.push(-1);
        }
      }
      rows.forEach((r, j) => {
        if (!used.has(j)) {
          r.dispose();
          for (const n of span(r)) if (n.parentNode === host) host.removeChild(n);
        }
      });
      for (let i = 0; i < list.length; i++) if (next[i] === null) next[i] = create(list[i], i);
      const keep = lis(oldIndex);
      let anchor = end;
      for (let i = list.length - 1; i >= 0; i--) {
        const r = next[i];
        if (!keep.has(i)) for (const n of span(r)) host.insertBefore(n, anchor);
        if (r.first) anchor = r.first;
      }
      rows = next;
    });
  });
}
