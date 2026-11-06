# JavaScript Interop

## Decision

**For now, K has no npm support and no JavaScript interop in user code.**

## Consequences

- **The stdlib wraps the web platform:** HTTP, storage, WebSockets, canvas, clipboard, media, crypto, workers, and the rest. Anything a developer would otherwise reach for in JavaScript must exist in the stdlib (see the stdlib topic in [README.md](README.md)).
- **Only the stdlib can call JavaScript.** It uses an internal `@Native` mechanism that user code can't access.
- **All application code is K,** so the compiler's guarantees (types, layering, the server/client boundary, reactivity) hold everywhere, with no untyped holes.

## Revisit later

Typed npm interop (bindings generated from `.d.ts` files) may be added later. Any future design should keep JavaScript calls behind services, as the layering rules do for protocols.
