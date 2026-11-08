/**
 * @module k:runtime
 *
 * The K client runtime: everything generated client code imports. Each
 * export is documented in its module; docs/compiler/runtime.md specifies
 * the contracts and the size budget.
 */

export {
  Computed,
  Signal,
  batch,
  catchError,
  computed,
  effect,
  flush,
  getOwner,
  isTracking,
  onCleanup,
  renderEffect,
  root,
  runWithOwner,
  scope,
  signal,
  untrack,
} from "./signals.js";
export { attr, branch, classList, clear, on, place, prop, style, template, text, when } from "./dom.js";
export { each } from "./list.js";
export { child, joinClasses, lifetime, loadsOf, markers, paramProps, slotMarkers, title, withQuery } from "./view.js";
export { hydrate, isHydrating, readData, separator, textNode, uid } from "./hydrate.js";
export { KPanic, checkInt, equals, intDiv, intRem, isPanic, panic, textOf, unitCase } from "./values.js";
export * from "./std.js";
export * from "./money.js";
export { Store, store } from "./store.js";
export { navigate, startRouter } from "./router.js";
export { DEFAULT_TIMEOUT_MS, rpc, rpcError, sessionMirror } from "./rpc.js";
export { createForm, fieldParser, lengthRule, rangeRule, requiredRule } from "./forms.js";
export { FAILED, LOADING, READY, RELOADING, createLoads } from "./load.js";
export * from "./services.js";
