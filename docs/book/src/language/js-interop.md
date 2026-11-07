# JavaScript interop

K compiles to JavaScript, but **K code doesn't call JavaScript, and there is no npm.** All application
code is K, so the compiler's guarantees hold everywhere: types, the server/client boundary, layering
and reactivity have no untyped holes, and a package can't smuggle a database call into the browser
bundle.

## The standard library wraps the platform

What you would reach for in JavaScript comes from the [standard library](../reference/stdlib.md)
instead, behind typed K APIs:

| Instead of | Use |
|---|---|
| `fetch` | an injected `HttpClient` ([Calling HTTP APIs](../apps/http.md)) |
| `localStorage` | an injected `LocalStorage` with typed `StorageKey`s ([Local storage](../apps/local-storage.md)) |
| `setTimeout`, `setInterval`, `Date.now()` | the injected `Clock` ([Time and timers](../apps/time.md)) |
| `crypto.subtle`, `crypto.randomUUID()` | `Hmac`, `Hash`, `SecureRandom`, `Uuid` ([Bytes and cryptography](../apps/crypto.md)) |
| `<input type="file">`, download links | the injected `Files` ([Importing and exporting files](../apps/files.md)) |
| drag-and-drop libraries | `k.drag` ([Drag and drop](../apps/drag-and-drop.md)) |
| `keydown` listeners | `KeyboardShortcut` ([Keyboard shortcuts](../apps/keyboard-shortcuts.md)) |
| `JSON.parse`, `JSON.stringify` | `Codable` and `Json.decode`, `Json.encode` |
| chart libraries | `k.charts` ([Charts](../apps/charts.md)) |

Platform access goes through **interfaces injected into services**, never ambient globals, so every
test replaces it with a hermetic double (`TestHttpClient`, `MemoryStorage`, `TestClock`, `TestFiles`)
and the compiler knows which side of the wire each call runs on.

## How the standard library calls JavaScript

Standard-library declarations whose implementation lives in the runtime are marked `@Native`: they have
no K body, and the compiler or the JavaScript runtime (`crates/k_runtime`) provides it.

```k
// crates/k_sema/stdlib/k/time/Clock.k
@Native
public service Clock {
    /// The current instant. Not reactive: reading it subscribes to nothing.
    var now: Date
```

`@Native` is internal to the standard library, in `k.internal`, which user code can't import.

## What isn't available

- **No `import` of npm packages, and no `node_modules`.** A K project has no `package.json`, and
  building one needs no Node.js.
- **No inline JavaScript and no `eval`.** Views are built only from K elements and components, so there
  is no element that runs a script.
- **No raw DOM handles.** Views are declarative: state changes update the elements that read it, and
  events reach your code through `.on(…)`.

> **Planned.** Typed npm interop, with bindings generated from `.d.ts` files, is deferred by the design
> and may be added later. Any design for it would keep JavaScript calls behind services, as the layering
> rules already do for protocols. Until then, a capability K lacks is a gap in the standard library.
