# Reactivity

K uses **compiled, fine-grained signals**, like Svelte 5 and Solid. There is no virtual DOM. The compiler
links each piece of state to the exact DOM nodes that read it, so when the state changes, those nodes
update and nothing else runs. There is no diffing.

Reactivity is **explicit**: only annotated fields are reactive.

```k
~import k.observe.Log
component Counter {
    @State var count: Int = 0
    @Derived let doubled: Int = count * 2

    @Effect
    func logChanges() {
        Log.info("count is now {count}")
    }

    func increment() {
        count += 1
    }

    view {
        button { "{count} × 2 = {doubled}" }
            .on(click: increment)
    }
}
```

| Annotation | Meaning |
|---|---|
| `@State` | A mutable reactive value. Writing it updates everything that depends on it. |
| `@Derived` | A read-only value, computed lazily from other reactive values. No dependency arrays. |
| `@Effect` | A side effect that re-runs when the values it read change. |

A plain `var` is ordinary, non-reactive data. Changing it never updates the view.

## `@State`

`@State` fields are always `var`. Assigning one, or mutating it in place, is a tracked write:

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/shop/Context.k
struct CartItem {
    let name: String
}

struct Address {
    var city: String
}

struct User {
    var address: Address

    static let sample = User(address: Address(city: "Oslo"))
}

struct Product {
    let price: Money
}

struct Draft implements Codable {
    let text: String
}

enum LookupError implements Error {
    case notFound
}

struct Profile {
    let name: String
}

service Users {
    func load(_ id: Int) async throws LookupError -> Profile {
        throw .notFound
    }
}

struct Place {
    let name: String
}

service Places {
    func search(_ text: String) async throws LookupError -> List<Place> {
        return []
    }
}

service Analytics {
    func send(_ page: String, _ info: String) {}

    func flush() {}
}
```
-->

```k
~component Cart {
@State var items: List<CartItem> = []
@State var user: User = .sample

func add(item: CartItem) {
    items.append(item)
}

func move() {
    user.address.city = "Bergen"
}
~    view {
~        text { "…" }
~    }
~}
```

Writes to nested fields of a struct are fine-grained: `user.address.city = "Bergen"` updates only the DOM
nodes that read `user.address.city`. This works because structs have
[value semantics](../language/declarations.md#value-semantics). [Class](../language/declarations.md#classes)
instances are different: in `@State`, only reassigning the reference is tracked.

## `@Derived`

A `@Derived` field is a value computed from other reactive values. It is recomputed only when one of them
changes, and only when something reads it:

```k
~component ProductFilter {
@State var maxPrice: Money = 50.usd
@Prop let products: List<Product>

@Derived let matches: List<Product> = products.filter { product => product.price <= maxPrice }
@Derived let summary: String = if matches.isEmpty { "No results" } else { "{matches.count} results" }
~    view {
~        text { "…" }
~    }
~}
```

There are no dependency arrays to keep in sync: the dependencies are whatever the expression reads.

- Writing to a `@Derived` field is a compile error.
- Writing to `@State` inside a `@Derived` computation is a compile error.

## `@Effect`

An effect is a function that runs after the component mounts and re-runs whenever a `@State` or
`@Derived` value **it actually read during its last run** changes:

```k
~component Editor {
~    @Prop let isLoggedIn: Bool
~    @State var draft: Draft = Draft(text: "")
~
~    func save(_ draft: Draft) {}
@Effect
func sync() {
    if isLoggedIn {           // always read
        save(draft)           // read only when isLoggedIn is true
    }
}
~    view {
~        text { "…" }
~    }
~}
```

Tracking happens at runtime, because a static scan would be wrong for conditional reads and for reads
inside helper functions. Here, `draft` is a dependency only while `isLoggedIn` is true.

The compiler also runs a static pass, for tooling:

- A warning appears when an effect reads no reactive values, because it would never re-run.
- A warning appears when state is read after an `await`, because those reads aren't tracked.
- The language server shows an effect's dependencies on hover, and a `@Derived` field's too (see
  [Editor setup](../getting-started/editor-setup.md)).

### Where effects run

- **Effects run only in the browser,** after the component has mounted, or after hydration for a
  server-rendered page. **They never run during server rendering,** and neither do timers started in
  them. Server-side work on a schedule is a [background job](jobs.md).
- Leaving a page, or unmounting a component, cancels its running effects and runs their `onCleanup`
  blocks.

### Effects handle their own errors

**An `@Effect` function can't be declared `throws`.** Nothing calls an effect, so nothing could catch its
error. It handles every error inside, with `do`/`catch` or `try … else`, and usually records it in state.
Declaring `throws` on an effect is a compile error with a quick fix that wraps the body in `do`/`catch`.
Event handlers follow the same rule.

### Explicit dependencies

`@Effect(on: […])` names the dependencies explicitly. Reads in the body are then ignored for tracking:

```k
~component ProfileCard {
~    @Inject let users: Users
~    @Prop let userId: Int
~    @State var profile: LoadState<Profile> = .idle
@Effect(on: [userId])
func loadUser() async {
    do {
        profile = .loaded(try await users.load(userId))
    } catch let error {
        profile = .failed(error)
    }
}
~    view {
~        text { "…" }
~    }
~}
```

`@Effect(on: [])` runs once after mount and never re-runs, which is how a page follows a long-lived
stream for as long as it is shown ([Server streaming](streaming.md)).

### Reading without subscribing, and cleaning up

```k
~component Tracker {
~    @Inject let analytics: Analytics
~    @Prop let page: String
~    @State var sessionInfo: String = ""
@Effect
func track() {
    analytics.send(page, untracked { sessionInfo })   // read without subscribing
    onCleanup { analytics.flush() }                    // before each re-run, and on unmount
}
~    view {
~        text { "…" }
~    }
~}
```

- `untracked { … }` reads values without making them dependencies.
- `onCleanup { … }` registers work to run before the effect re-runs, and when the component unmounts.

Timers started in an effect should be cancelled in `onCleanup`:

```k
~import k.time.Clock
~
~component Forecast {
~    @Prop let placeId: Int
~
~    func reload() {}
@Inject let clock: Clock

@Effect(on: [placeId])
func refreshPeriodically() {
    let timer = clock.every(10.minutes) { reload() }

    onCleanup { timer.cancel() }
}
~    view {
~        text { "…" }
~    }
~}
```

The compiler warns when a timer handle returned inside an effect is discarded, because the timer would
outlive the component. See [Time and timers](time.md).

## Async effects are cancelled, not raced

When a dependency changes while an async effect is still running, **the previous run is cancelled
automatically**. Whatever it was awaiting is abandoned, and it never resumes. This removes the classic
race condition where an old, slow response overwrites a newer one. Cancellation isn't an error: a
cancelled `await` (a request, a `clock.sleep`) never resumes, so there is nothing to catch.

Cancellation also makes debouncing trivial. This effect waits for typing to pause before searching,
because each keystroke cancels the run that is still sleeping. The sleep comes from the injected
`Clock`, as every timer does:

```k
~import k.time.Clock
~
~component Search {
~    @Inject let clock: Clock
~    @Inject let places: Places
~    @State var query: String = ""
~    @State var results: LoadState<List<Place>> = .idle
@Effect
func search() async {
    let text = query

    await clock.sleep(300ms)              // a new keystroke cancels this run

    do {
        results = .loaded(try await places.search(text))
    } catch let error {
        results = .failed(error)
    }
}
~    view {
~        text { "…" }
~    }
~}
```

## State in services

`@State`, `@Derived` and `@Effect` work the same in [services](services.md), which is how K handles
shared state: there is no separate store library. A component that reads `cart.items` from an injected
`CartService` updates whenever the service's state changes.

## Why explicit annotations?

- A reader can tell from the declaration alone which values can change and what depends on them.
- The compiler knows exactly which fields are state, which is what makes state-preserving hot module
  replacement possible (**Planned** for a later compiler release).
- Ordinary data stays ordinary: nothing pays for reactivity it doesn't need.
