# Reactivity

## Decision

K uses **compiled fine-grained signals** with no virtual DOM, like Svelte 5 and Solid. The compiler links each piece of state to the exact DOM nodes that read it, so there is no diffing.

Reactivity is **explicit**. Only annotated fields are reactive:

```k
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
}
```

| Annotation | Meaning |
|---|---|
| `@State` | Mutable reactive value. Writing it updates dependents. |
| `@Derived` | Read-only, computed lazily from other reactive values. No dependency arrays. |
| `@Effect` | A side effect that re-runs when the values it read change. |

A plain `var` is ordinary, non-reactive data.

## Rules

- Writing to a `@Derived` field is a compile error.
- Writing to `@State` inside a `@Derived` computation is a compile error.
- **A `@Derived` field can't depend on itself**, directly (`c = c`) or through other `@Derived` fields
  (`a = b + 1`, `b = a + 1`): no value could be computed first. It is a compile error at every field of
  the cycle. Reads inside `untracked { … }` count too: they don't subscribe, but the value is still
  needed.

## How effects track dependencies

An effect re-runs when any `@State` or `@Derived` value **it actually read during its last run** changes.

- **Tracking happens at runtime.** A static scan of the body would be wrong for conditional reads and for reads inside helper functions:

  ```k
  @Effect
  func sync() {
      if isLoggedIn {           // always read
          save(draft)           // read only when isLoggedIn is true
      }
  }
  ```

- **The compiler also runs a static pass, for tooling:**
  - The IDE shows an effect's dependencies on hover.
  - A warning appears when an effect reads no reactive values, because it would never re-run.
  - A warning appears when state is read after an `await`, because those reads are not tracked.

## Escape hatches

```k
@Effect(on: [userId])                 // explicit dependencies; reads in the body are ignored
func loadUser() async {
    do {
        profile = .loaded(try await users.load(userId))
    } catch let error {
        profile = .failed(error)
    }
}

@Effect
func track() {
    Analytics.send(page, untracked { sessionInfo })   // read without subscribing
    onCleanup { Analytics.flush() }                    // before each re-run and on unmount
}
```

`@Effect(on: [])` runs once after mount and never re-runs. It is how a page follows a server stream for as long as it is shown ([29-server-streaming.md](29-server-streaming.md)).

## Async effects

When a dependency changes while an async effect is still running, the previous run is **cancelled automatically**. This removes the race condition that `useEffect` plus `fetch` causes in React. Cancellation is not an error: a cancelled `await` (a request, a `clock.sleep`) never resumes, so there is nothing to catch.

## Async work is owned

**Decision:** work that a component or page starts is owned by it, and is cancelled when it unmounts, the way a Swift structured task is cancelled with its scope.

- **Every async run has an owner.** An async handler (a function or a lambda passed to `.on(…)`, an event handler written by the listening component, a `form.handle(…)` action, an async function or lambda given to a child as a prop) runs under the component that wrote it. An `@Effect` run is owned by its effect and a `@Load` run by its load group, and both of those belong to the component. The owner is cancelled when the component unmounts or the page is left. A run is also cancelled when it is superseded: the effect re-runs, or the load group starts over.
- **The owner's cancellation reaches the I/O.** Every rpc and every `HttpClient` call made from a run, directly or through services, carries the owner's `AbortSignal`, and is aborted when the owner is cancelled. So is `clock.sleep`.
- **No code after an `await` runs on a cancelled owner.** A promise can settle while the owner is still live, and the owner can be cancelled before the body resumes. So after each `await`, and on entering each `catch`, the generated code checks the owner and stops the body if it was cancelled. Each iteration of a `for await` is checked the same way. A body therefore never writes the state of a disposed page, never navigates from it, and never reads its narrowed `session.user` after the session ended ([26-auth-and-sessions.md](26-auth-and-sessions.md#in-the-browser)).
- **Cancellation is silent.** It is not an error that user code sees. A stopped body unwinds without running any `catch` clause, `try … else` fallback or other K code, and nothing reports it: an effect, a load, a handler and a dropped event handler all ignore it. K has no `defer` or `finally`. Cleanup that must run when an owner goes away is an `onCleanup` block, and those still run, before each re-run and on unmount.
- **A lambda passed to a K function is not a new owner.** `retry { await api.save() }` runs the lambda under the caller's own run, which `retry` passes on, so cancelling the caller cancels the lambda.

Why: a page that is left must not keep acting. Without ownership, a slow response arriving after navigation could write state nobody sees, navigate the user somewhere unexpected, or crash a page that `requires Authenticated` by reading a user who just signed out. Cooperative cancellation at each `await` needs no locks and adds no error case for the developer to handle, and ties the lifetime of async work to the lifetime of the UI that started it.

## Where effects run

- **Effects run only in the browser,** after the component has mounted (after hydration, for a server-rendered page). **They never run during server rendering,** and neither do timers started in them. Server-side work on a schedule is a background job ([30-background-jobs.md](30-background-jobs.md)).
- Leaving a page, or unmounting a component, cancels its running effects, handlers and loads ([above](#async-work-is-owned)) and runs their `onCleanup` blocks.

## Effects handle their own errors

- **An `@Effect` function can't be declared `throws`.** Nothing calls an effect, so nothing could catch its error. It handles every error inside, with `do`/`catch` or `try … else`, and usually records it in state, as `loadUser` does above.
- Declaring `throws` on an effect is a compile error, with a quick fix that wraps the body in `do`/`catch`. Event handlers follow the same rule ([04-components.md](04-components.md)).

## Nested state

Mutating a nested field of a `@State` struct is fine-grained: only DOM nodes that read that path update. See [08-types.md](08-types.md).
