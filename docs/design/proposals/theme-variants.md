# Proposal: Switching Theme Variants at Runtime

> **Status: proposal.** Not yet accepted into the design. First used by `examples/todos`.

## Problem

[07-themes-and-units.md](../07-themes-and-units.md) declares variants (`variant dark { … }`) and says switching is `Theme.use(.dark)`, a single class swap. It doesn't say where `Theme` comes from, what type `Theme.use` takes, how to switch *back* to the base tokens (which have no variant name), or how code and tests read the active variant.

## Proposal

### `Theme` lives in `k.ui`

```k
import k.ui.Theme

Theme.use(.dark)
Theme.use(.base)

expect Theme.current == .dark
```

| Member | Meaning |
|---|---|
| `Theme.use(variant)` | takes a `ThemeVariant` (unlabeled, as in [07-themes-and-units.md](../07-themes-and-units.md)) and makes it active by swapping one class on the document root. Nothing re-renders. |
| `Theme.current: ThemeVariant` | the active variant. It is reactive, so views and effects that read it update when it changes. |

### `ThemeVariant` is generated

- The compiler generates an enum, `ThemeVariant`, from the app's single `theme` declaration. It has **`.base`** for the tokens as declared, and one case per `variant` (`.dark`, `.highContrast`, …).
- Like the generated `Page` enum ([12-routing.md](../12-routing.md)), it needs no import, and an unknown variant is a compile error at the call site.
- `base` can't be used as a variant name, because it is the name for "no variant".

### Who calls `Theme.use`

- **Keep the user's choice in a service and apply it with an `@Effect`** in the page or layout that is always mounted:

  ```k
  @Effect
  func applyColorScheme() {
      Theme.use(if appearance.scheme == .dark { .dark } else { .base })
  }
  ```

  The service stays free of view concerns and easy to test, and the effect re-runs whenever the choice changes, including when the service restores a saved choice.
- **During server rendering**, effects don't run ([timers-and-clock.md](timers-and-clock.md)), so the page renders with `.base`. An app that persists the choice in local storage and must avoid a flash of the wrong scheme renders on the client ([app-structure.md](app-structure.md), `@Rendering(.client)`).

### Tests

Each test starts with `Theme.current == .base`, like every other piece of per-test state ([testing-support.md](testing-support.md)).

## Why

- A generated enum keeps variant names checked at compile time, like pages and class references.
- Naming the base `.base` makes switching back explicit, with no `nil` or "reset" call that reads differently from switching forward.
- Exposing `Theme.current` reactively lets components react to the scheme (for example a chart that changes palettes) without duplicating the state.

## Open questions

- **Following the operating system.** A variant could declare `variant dark matches .prefersDark`, so that the stdlib applies it from `prefers-color-scheme` until `Theme.use` is called. The example implements only an explicit switch.
- **Scoped variants**, applying a variant to one subtree (for example a dark sidebar in a light app), would need a `.theme(.dark)` modifier.
