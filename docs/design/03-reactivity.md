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
    user = try await users.load(userId)
}

@Effect
func track() {
    Analytics.send(page, untracked { sessionInfo })   // read without subscribing
    onCleanup { Analytics.flush() }                    // before each re-run and on unmount
}
```

## Async effects

When a dependency changes while an async effect is still running, the previous run is **cancelled automatically**. This removes the race condition that `useEffect` plus `fetch` causes in React.

## Nested state

Mutating a nested field of a `@State` struct is fine-grained: only DOM nodes that read that path update. See [08-types.md](08-types.md).
