/**
 * @module k:runtime/signals
 *
 * Fine-grained reactivity for K: signals, lazy computed values, effects,
 * batching, untracked reads, cleanups and the owner tree.
 *
 * The algorithm is push-pull with three node states (CLEAN, CHECK, DIRTY):
 *
 * - writing a signal marks its direct observers DIRTY and everything
 *   downstream of them CHECK ("maybe stale"), and schedules the effects it
 *   reaches;
 * - reading a computed value that is not CLEAN first brings its sources up
 *   to date, and recomputes only if one of them actually changed value
 *   (`Object.is`), so an unchanged intermediate value stops propagation;
 * - dependencies are re-collected on every run, so conditional reads are
 *   tracked exactly (docs/design/03-reactivity.md).
 *
 * Scheduling: re-runs of effects are queued and flushed in a microtask, or
 * synchronously at the end of the outermost `batch`. A flush runs render
 * effects (DOM bindings) before user effects (`@Effect`), each in creation
 * order, which puts parents before children. The contract is specified in
 * docs/compiler/runtime.md, "Signals".
 */

const CLEAN = 0;
const CHECK = 1;
const DIRTY = 2;

/** The node whose run is collecting dependencies, or `null`. */
let observer = null;
/** The sources read by `observer` in its current run. */
let newSources = null;
/** The owner that new nodes and cleanups attach to, or `null`. */
let owner = null;
/** Nesting depth of `batch`. */
let batchDepth = 0;
/** `true` while a microtask flush is scheduled. */
let flushScheduled = false;
/** `true` while `flush` runs. */
let flushing = false;
/** Monotonic id; creation order of effects. */
let nextId = 0;
/** Effects waiting to re-run, by kind. */
const renderQueue = [];
const userQueue = [];

/**
 * An owner: a scope that owns child nodes and cleanups. Effects and
 * computed values are owners of what they create while running; `root`
 * creates a detached owner.
 * @private
 */
class Owner {
  constructor(parent) {
    /** @type {Owner|null} */
    this.parent = parent;
    /** @type {Array<Owner>|null} */
    this.children = null;
    /** @type {Array<() => void>|null} */
    this.cleanups = null;
    /** @type {((error: unknown) => void)|null} */
    this.onError = null;
    /** @type {boolean} */
    this.disposed = false;
    if (parent !== null && !parent.disposed) {
      (parent.children ??= []).push(this);
    }
  }

  /** Disposes the children, then runs the cleanups in reverse order. */
  reset() {
    const children = this.children;
    this.children = null;
    if (children !== null) {
      for (let i = children.length - 1; i >= 0; i--) children[i].dispose();
    }
    const cleanups = this.cleanups;
    this.cleanups = null;
    if (cleanups !== null) {
      for (let i = cleanups.length - 1; i >= 0; i--) {
        try {
          cleanups[i]();
        } catch (error) {
          handleError(this, error);
        }
      }
    }
  }

  /** Disposes this owner and its subtree. Idempotent. */
  dispose() {
    if (this.disposed) return;
    this.reset();
    this.disposed = true;
  }
}

/**
 * A reactive node: a signal (no `fn`), a computed value or an effect.
 * @private
 */
class Node extends Owner {
  constructor(parent, fn, value, kind) {
    super(fn === null ? null : parent);
    this.id = nextId++;
    this.fn = fn;
    this.value = value;
    /** 0 signal, 1 computed, 2 render effect, 3 user effect. */
    this.kind = kind;
    this.state = fn === null ? CLEAN : DIRTY;
    /** @type {Array<Node>} */
    this.sources = [];
    /** @type {Set<Node>} */
    this.observers = new Set();
    /** @type {AbortController|null} */
    this.abort = null;
    this.queued = false;
  }

  dispose() {
    if (this.disposed) return;
    super.dispose();
    if (this.abort !== null) {
      this.abort.abort();
      this.abort = null;
    }
    for (const source of this.sources) source.observers.delete(this);
    this.sources = [];
    this.observers.clear();
  }
}

function handleError(from, error) {
  for (let o = from; o !== null; o = o.parent) {
    if (o.onError !== null) {
      o.onError(error);
      return;
    }
  }
  // No boundary: surface it asynchronously so one failing effect does not
  // stop the others in the same flush.
  queueMicrotask(() => {
    throw error;
  });
}

function track(node) {
  if (observer !== null && newSources !== null && !newSources.includes(node)) {
    newSources.push(node);
  }
}

function markStale(node, state) {
  if (node.state >= state) return;
  if (node.state === CLEAN && node.kind >= 2) schedule(node);
  node.state = state;
  for (const o of node.observers) markStale(o, CHECK);
}

function schedule(effect) {
  if (effect.queued || effect.disposed) return;
  effect.queued = true;
  (effect.kind === 2 ? renderQueue : userQueue).push(effect);
  if (batchDepth === 0 && !flushing && !flushScheduled) {
    flushScheduled = true;
    queueMicrotask(flushFromMicrotask);
  }
}

function flushFromMicrotask() {
  flushScheduled = false;
  flush();
}

function refresh(node) {
  if (node.state === CHECK) {
    for (const source of node.sources) {
      if (source.fn !== null) refresh(source);
      if (node.state === DIRTY) break;
    }
  }
  if (node.state === DIRTY) run(node);
  node.state = CLEAN;
}

function run(node) {
  if (node.disposed) return;
  node.reset();
  if (node.abort !== null) {
    node.abort.abort();
    node.abort = null;
  }
  const previousObserver = observer;
  const previousSources = newSources;
  const previousOwner = owner;
  observer = node;
  newSources = [];
  owner = node;
  let result;
  try {
    if (node.kind >= 2 && node.fn.length > 0) {
      node.abort = new AbortController();
      result = node.fn(node.abort.signal);
    } else {
      result = node.fn(node.value);
    }
  } catch (error) {
    // A computed value rethrows to its reader and stays DIRTY, so the next
    // read retries; an effect reports to its error boundary.
    if (node.kind === 1) throw error;
    handleError(node, error);
    result = undefined;
  } finally {
    relink(node, newSources);
    observer = previousObserver;
    newSources = previousSources;
    owner = previousOwner;
  }
  if (node.kind >= 2) {
    if (result !== null && typeof result === "object" && typeof result.then === "function") {
      // An async effect: report a failure, ignore cancellation.
      const signal = node.abort === null ? null : node.abort.signal;
      result.then(undefined, (error) => {
        if (signal === null || !signal.aborted) handleError(node, error);
      });
    }
    return;
  }
  if (!Object.is(result, node.value)) {
    node.value = result;
    for (const o of node.observers) o.state = DIRTY;
  }
}

function relink(node, sources) {
  for (const old of node.sources) {
    if (!sources.includes(old)) old.observers.delete(node);
  }
  for (const source of sources) source.observers.add(node);
  node.sources = sources;
}

/**
 * Runs every queued effect until none is left: render effects first, then
 * user effects, each in creation order. Called automatically in a microtask
 * after a write and at the end of the outermost `batch`; tests call it to
 * observe effects synchronously.
 */
export function flush() {
  if (flushing) return;
  flushing = true;
  try {
    let runs = 0;
    while (renderQueue.length > 0 || userQueue.length > 0) {
      // One round: every queued render effect, else every queued user
      // effect, in creation order. Effects queued meanwhile run in the
      // next round, so render effects always settle before user effects.
      const queue = renderQueue.length > 0 ? renderQueue : userQueue;
      const round = queue.splice(0, queue.length);
      if (round.length > 1) round.sort((a, b) => a.id - b.id);
      for (const effect of round) {
        effect.queued = false;
        if (!effect.disposed) refresh(effect);
      }
      runs += round.length;
      if (runs > 100_000) {
        // Drop the cycle, so a later flush doesn't spin again.
        for (const e of renderQueue) e.queued = false;
        for (const e of userQueue) e.queued = false;
        renderQueue.length = 0;
        userQueue.length = 0;
        throw new Error("k: effects keep re-triggering each other");
      }
    }
  } finally {
    flushing = false;
  }
}

/**
 * A readable, writable reactive value.
 * @template T
 */
export class Signal {
  /** @param {Node} node @private */
  constructor(node) {
    /** @private */
    this.node = node;
  }

  /** Returns the value and subscribes the running computation to it. @returns {T} */
  get() {
    track(this.node);
    return this.node.value;
  }

  /** Returns the value without subscribing. @returns {T} */
  peek() {
    return this.node.value;
  }

  /**
   * Replaces the value. Writing a value that is `Object.is`-equal to the
   * current one notifies nobody.
   * @param {T} value
   */
  set(value) {
    const node = this.node;
    if (Object.is(node.value, value)) return;
    node.value = value;
    for (const o of node.observers) markStale(o, DIRTY);
  }

  /** Sets the value to `fn(current)`. @param {(value: T) => T} fn */
  update(fn) {
    this.set(fn(this.node.value));
  }
}

/**
 * Creates a signal holding `value`.
 * @template T
 * @param {T} value
 * @returns {Signal<T>}
 */
export function signal(value) {
  return new Signal(new Node(null, null, value, 0));
}

/**
 * A lazily computed, cached reactive value (`@Derived`).
 * @template T
 */
export class Computed {
  /** @param {Node} node @private */
  constructor(node) {
    /** @private */
    this.node = node;
  }

  /** Brings the value up to date if needed, subscribes and returns it. @returns {T} */
  get() {
    const node = this.node;
    if (node.disposed) return node.value;
    track(node);
    refresh(node);
    return node.value;
  }

  /** Returns the up-to-date value without subscribing. @returns {T} */
  peek() {
    return untrack(() => this.get());
  }
}

/**
 * Creates a computed value. `fn` runs on the first read and again on a read
 * after one of the values it read changed; it receives its previous value.
 * @template T
 * @param {(previous: T|undefined) => T} fn
 * @returns {Computed<T>}
 */
export function computed(fn) {
  return new Computed(new Node(owner, fn, undefined, 1));
}

/**
 * Creates a render effect: runs `fn` now, and again (batched, in a flush)
 * whenever a value it read during its last run changes. Used for DOM
 * bindings, which must be in place when a component's factory returns.
 * Returns a function that disposes the effect.
 * @param {(signal?: AbortSignal) => unknown} fn
 * @returns {() => void}
 */
export function renderEffect(fn) {
  const node = new Node(owner, fn, undefined, 2);
  refresh(node);
  return () => node.dispose();
}

/**
 * Creates a user effect (`@Effect`): its first run is queued (it runs after
 * the current render, in the next flush), and it re-runs when a value it
 * read changes. If `fn` declares a parameter it receives an `AbortSignal`
 * that is aborted when the run is superseded or the effect is disposed; a
 * promise it returns is awaited only to report failures, never to order
 * runs (docs/design/03-reactivity.md, "Async effects").
 * Returns a function that disposes the effect.
 * @param {(signal?: AbortSignal) => unknown} fn
 * @returns {() => void}
 */
export function effect(fn) {
  const node = new Node(owner, fn, undefined, 3);
  schedule(node);
  return () => node.dispose();
}

/**
 * Runs `fn` with writes batched: effects run once, synchronously, when the
 * outermost batch ends. Returns `fn`'s result.
 * @template T
 * @param {() => T} fn
 * @returns {T}
 */
export function batch(fn) {
  batchDepth++;
  try {
    return fn();
  } finally {
    endBatch();
  }
}

function endBatch() {
  batchDepth--;
  if (batchDepth === 0 && !flushing && (renderQueue.length > 0 || userQueue.length > 0)) {
    // The outermost batch (an event handler, a navigation) ends: flush now,
    // so the DOM is current when the handler returns. Writes outside any
    // batch are flushed in a microtask instead.
    flush();
  }
}

/**
 * Runs `fn` without subscribing the running computation to what it reads
 * (`untracked { … }`).
 * @template T
 * @param {() => T} fn
 * @returns {T}
 */
export function untrack(fn) {
  const previousObserver = observer;
  const previousSources = newSources;
  observer = null;
  newSources = null;
  try {
    return fn();
  } finally {
    observer = previousObserver;
    newSources = previousSources;
  }
}

/**
 * Registers `fn` to run before the current owner re-runs and when it is
 * disposed (`onCleanup { … }`). Outside any owner it does nothing.
 * @param {() => void} fn
 */
export function onCleanup(fn) {
  if (owner !== null) (owner.cleanups ??= []).push(fn);
}

/**
 * Creates a detached owner and runs `fn(dispose)` inside it. Everything
 * created inside belongs to the root and is disposed by `dispose`.
 * @template T
 * @param {(dispose: () => void) => T} fn
 * @returns {T}
 */
export function root(fn) {
  const scope = new Owner(null);
  const previousOwner = owner;
  const previousObserver = observer;
  const previousSources = newSources;
  owner = scope;
  observer = null;
  newSources = null;
  try {
    return fn(() => scope.dispose());
  } finally {
    owner = previousOwner;
    observer = previousObserver;
    newSources = previousSources;
  }
}

/**
 * Creates a child owner of the current one and runs `fn(dispose)` inside it
 * (a component instance, a branch of `when`). Disposing the parent disposes
 * it too.
 * @template T
 * @param {(dispose: () => void) => T} fn
 * @returns {T}
 */
export function scope(fn) {
  const child = new Owner(owner);
  const previousOwner = owner;
  owner = child;
  try {
    return fn(() => child.dispose());
  } finally {
    owner = previousOwner;
  }
}

/**
 * Installs `handler` as the error boundary of a new child owner and runs
 * `fn` inside it: errors thrown by effects created inside reach `handler`
 * instead of the global error reporter (`ErrorBoundary`).
 * @template T
 * @param {() => T} fn
 * @param {(error: unknown) => void} handler
 * @returns {T}
 */
export function catchError(fn, handler) {
  return scope(() => {
    owner.onError = handler;
    return fn();
  });
}

/**
 * Returns `true` if a computation is currently collecting dependencies.
 * @returns {boolean}
 */
export function isTracking() {
  return observer !== null;
}

/**
 * Returns the current owner (a component scope, block branch or effect
 * run), for runtime code that creates scopes later from inside an effect.
 * @returns {object|null}
 */
export function getOwner() {
  return owner;
}

/**
 * Runs `fn` with `o` as the current owner and without tracking: what `fn`
 * creates belongs to `o` and is disposed with it.
 * @template T
 * @param {object|null} o an owner returned by {@link getOwner}
 * @param {() => T} fn
 * @returns {T}
 */
export function runWithOwner(o, fn) {
  const previousOwner = owner;
  const previousObserver = observer;
  const previousSources = newSources;
  owner = o;
  observer = null;
  newSources = null;
  try {
    return fn();
  } finally {
    owner = previousOwner;
    observer = previousObserver;
    newSources = previousSources;
  }
}
