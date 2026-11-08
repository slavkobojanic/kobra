# Compilation Model

## Decision

K is **compiled ahead of time** to JavaScript.

- The compiler reads `.k` files, type-checks them, and outputs plain JavaScript, CSS and Protobuf definitions.
- Types are erased at runtime. The shipped runtime is small.
- Development uses a dev server with **hot module replacement (HMR)**. Edits apply without a full reload, and component state is preserved.

## Why

- Ahead-of-time compilation enables the static analysis the rest of the design depends on: fine-grained reactivity, compile-time dependency injection, typed class references, typed URLs, and layering rules.
- An interpreter shipped to the browser would mean a slow, large bundle competing with JavaScript on its home turf.

## Consequences

- **Dependency injection and annotations are resolved at compile time.** The compiler generates the wiring code. There is no runtime reflection like Java's, and the output is tree-shakeable.
- **State-preserving HMR is possible because state is declared explicitly** (`@State`). The compiler knows exactly which fields to carry across a reload.
- **The compiler splits each app into a server bundle and a client bundle,** and enforces the boundary (see [10-services.md](10-services.md)).
- **An app ships only the runtime it uses.** Every client gets one runtime chunk (signals, the renderer, hydration, the router, the RPC client, `@Load` and the common stdlib bodies). Code that only some apps need ships as a chunk of its own, only to the apps that use it: charts, drag and drop, files, keyboard shortcuts, server streams, `HttpResponse`, `@Form` support and the exact `Decimal`/`Money` types. When the runtime chunk nears its size budget, the next such feature moves out rather than the budget going up ([performance.md §11](../compiler/performance.md#11-stage-3-runtime-and-server)).
