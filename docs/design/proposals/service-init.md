# Proposal: Service Initialization with `init()`

> **Status: proposal.** Not yet accepted into the design. First used by `examples/todos`.

## Problem

A service often needs to set up its state from a dependency once, as soon as it exists: restore saved data from storage, read a cookie, seed a cache. [10-services.md](../10-services.md) shows injected fields and `@State` initializers, but no place for setup logic that writes more than one field or handles errors. Workarounds are poor:

- A `@State` initializer can call a function (`@State var items = restored()`), but it can set only that one field, so a failure has nowhere to go.
- An `@Effect` that only writes state reads nothing, so the compiler warns that it never re-runs ([03-reactivity.md](../03-reactivity.md)). Effects are also for keeping things in sync, not for one-time setup.
- Asking every page to call `service.restore()` spreads the service's responsibility across its callers.

## Proposal

**A service may declare one parameterless `init()`.** It runs once per instance, after every `@Inject` field is resolved and every field initializer has run, and before the instance is given to anything that injects it.

```k
public service TodoService {
    @Inject let storage: LocalStorage

    @State public var items: List<Todo> = []
    @State public var problem: StorageProblem? = nil

    /// Restores the saved todos as soon as the service is created.
    init() {
        restore()
    }

    public func restore() { ... }
}
```

### Rules

- **No parameters.** A service's inputs are its injected dependencies, and the compiler supplies them. `init(...)` with parameters is a compile error.
- **Synchronous and non-throwing.** `init()` can't be `async` or `throws`: nothing could await it or catch its error, because the instance is created by the container. Errors are handled inside and recorded in state, as `TodoService` does with `problem`. Async data belongs in `@Load` ([14-async-data.md](../14-async-data.md)) or in a method the caller awaits.
- **It may write the service's own `@State`**, like any of its methods ([10-services.md](../10-services.md)). Writing `@Derived` is a compile error, as everywhere.
- **It may call injected services.** The dependency graph is acyclic ([10-services.md](../10-services.md)), so a dependency's `init()` has always finished first.
- **When it runs follows the scope.** `@Scope(.app)`: when the app starts, or when the first injector is created. `@Scope(.component)`: once per injecting subtree. In tests, every test gets a fresh container ([testing-support.md](testing-support.md)), so `init()` runs again for each test. A test that needs different starting data configures the fake, then calls the service's own public method (here `restore()`) again.
- **Only services.** Components and pages already have field initializers for setup and `@Effect` for side effects. `init()` there would be a second way to do the same thing.
- **Placement:** the formatter places `init()` after the fields and before the other functions.

## Why

- `init` is Swift's established name for this, and K takes `func`, `let` and `var` from Swift as well ([01-principles.md](../01-principles.md)).
- Running after injection is what makes it useful. It is the counterpart of Spring's `@PostConstruct`, but a fixed name needs no annotation, and there can be only one.
- Keeping it synchronous and non-throwing means creating a service can never half-fail, and the container stays simple and compile-time generated.

## Open questions

- Is a teardown counterpart needed for `@Scope(.route)` and `@Scope(.component)` services, for example to cancel timers? `onCleanup` inside `init()` could be the answer, to match effects.
