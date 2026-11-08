# Components

## File shape

A component is a **single declaration** whose body holds its fields, functions, view and styles. This is Vue's single-file component without the XML sections.

```k
// Counter.k

public component Counter {
    @Prop let label: String
    @Prop let start: Int = 0
    @Emits let changed: Event<Int>

    @State var count: Int = start

    func increment() {
        count += 1
        changed.emit(count)
    }

    view {
        box {
            text { "{label}: {count}" }

            button { "Add" }
                .on(click: increment)
        }
    }

    style {
        .label { fontSize: .lg; }
    }
}
```

- A component has exactly **one `view { }`** block and **at most one `style { }`** block. The order of members in the body is free.
- **Several declarations per file** are allowed, but at most one is `public`, and it must match the file name (`Counter.k` holds `public component Counter`). The others are helpers visible within the module. See [16-modules.md](16-modules.md).

## Public API

| Annotation | Meaning |
|---|---|
| `@Prop` | One-way input from the parent. Read-only in the child. |
| `@Binding` | Two-way input. The child may write, and the parent's state changes. |
| `@Emits` | An event the component fires: something that happened, not a value that changed. |
| `@Slot` | Content passed in by the parent (see below). |
| `@Stylable` | A class hook a parent may style (see [06-styles.md](06-styles.md)). |

## Required arguments are named; optional ones are chained

- A prop **without** a default is required and is passed as a named argument.
- A prop **with** a default, or an optional prop (`T?`), is set through a **generated chained method**.

```k
component Stepper {
    @Binding var value: Int          // required → named argument
    @Prop let step: Int = 1          // has a default → .step()
    @Prop let max: Int? = nil        // optional → .max()
}

Stepper(value: bind quantity)
    .step(5)
    .max(99)
```

Rules:

- The compiler generates the chained methods. There are no hand-written builders.
- Setting the same option twice is a compile error.
- Optional attributes are **order-independent**.
- **Modifiers** (cross-cutting behaviors such as `.tooltip()`, `.animate()`, `.track()`) are chained too, but **apply in the order written**.
- A prop can't share a name with a registered modifier. That is a compile error at the prop declaration.
- **A `Bool` prop with a default is chained with its value:** `.dashed(true)`, `.disabled(isSubmitting)`. There are no zero-argument flag methods ([01-principles.md](01-principles.md)).
- **The attributes of built-in elements are not modifiers.** `.label`, `.placeholder`, `.type`, `.pressed`, `.expanded` and `.role` are generated per element from the element's typed definition, like any optional attribute ([05-views.md](05-views.md)). They don't reserve names, so `@Prop let label: String` is fine. A component that wants an accessible name declares its own `label` prop and applies it to an element inside.

## Two-way binding

Two-way state is **opt-in on both sides**: `@Binding` in the child, and `bind` at the call site.

```k
component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1

    view {
        box {
            button { "−" }
                .on(click: { value -= step })

            text { "{value}" }

            button { "+" }
                .on(click: { value += step })
        }
    }
}

component Cart {
    @State var quantity: Int = 1

    view {
        Stepper(value: bind quantity)

        text { "Total: {quantity * price}" }
    }
}
```

Why it isn't inferred from writes:

- A component's public API would change with its implementation. One write added during a refactor would silently give the child write access.
- Hidden two-way data flow is what sank AngularJS. `bind` at the call site makes it visible.

Compiler checks:

- A child writing to a `@Prop` → error, with a quick fix to change it to `@Binding`.
- Passing `bind x` where `x` isn't `@State` or `@Binding` → error. Literals and `@Derived` values can't be bound.
- Passing `quantity` without `bind` to a `@Binding` → error, with a quick fix to add `bind`.

Bindings work on nested fields (`bind user.address.city`). Custom bindings:

```k
Binding(get: { celsius * 9 / 5 + 32 }, set: { f => celsius = (f - 32) * 5 / 9 })
```

`@Emits` remains for events (submitted, closed, deleted) rather than state.

## Events

**A parent listens to a component's event with the same chained `.on(name: handler)` used for element events,** where `name` is the `@Emits` field:

```k
component Counter {
    @Emits let changed: Event<Int>
    @State var count: Int = 0

    func increment() {
        count += 1
        changed.emit(count)
    }

    view {
        button { "Add" }
            .on(click: increment)
    }
}

Counter()
    .on(changed: { value => Log.info("count is now {value}") })
```

- **The handler's type follows the event's payload.** For `Event<T>` it is a function taking a `T`: a lambda with one parameter, or a function passed by name (`.on(changed: record)`). An event without a payload is declared `Event<Void>`, fired with `emit()`, and handled by a lambda without parameters (`.on(closed: { isOpen = false })`).
- **One event per `.on(…)` call,** for elements and components alike. `.on(click: f, keyDown: g)` is a compile error with a fix that splits it into two calls. Listening to the same event twice on one call site is a compile error, like setting any option twice.
- **Listening is optional.** An event nobody listens to does nothing, so events are never required arguments.
- **Unknown event names are compile errors,** with a hint listing the component's `@Emits` fields (or the element's events).
- `emit` can be called only from inside the declaring component. A parent can't fire a child's event.

### Handlers

- **Handlers may be `async`.** The element or component doesn't wait for them. **An async handler is owned by the component that wrote it** ([03-reactivity.md](03-reactivity.md#async-work-is-owned)): its rpcs and `HttpClient` calls are aborted when that component unmounts or its page is left, and the handler stops at its next `await`, so no code after the `await` runs on the disposed component. The stop is silent: no `catch` clause runs and nothing is reported.
  - This holds for every kind of handler: a function or lambda passed to an element's `.on(…)`, an async lambda handling a child's `@Emits` event (owned by the listening parent, not the child that fires it), a `form.handle(…)` action, and an async function or lambda given to a child as a prop (owned by the parent that wrote it, even though the child calls it).
- **A handler can't be declared `throws`.** Nothing waits for it, so nothing could catch its error. It handles every error inside, usually by recording it in state. Passing a throwing function to `.on(…)` is a compile error, with a quick fix that wraps the function's body in `do`/`catch`. `@Effect` follows the same rule.
- A form's submit handler is usually wrapped by `form.handle(…)`, which may receive an action that throws `ValidationError` ([15-forms-and-validation.md](15-forms-and-validation.md)). The handler `form.handle` returns doesn't throw.

Why one form for both: a reader sees `.on(…)` and knows it is an event, whatever emits it. Events stay distinct from `@Binding`, which is for state the parent shares.

## Slots

A component accepts content through **typed `@Slot` props**:

```k
component Card {
    @Prop let title: String
    @Prop let items: List<CartItem>
    @Slot let content: View                  // required, unnamed → the trailing block
    @Slot let footer: View? = nil            // optional → chained
    @Slot let row: (CartItem) -> View        // scoped slot: the child passes data back

    view {
        article {
            heading(level: 2) { title }

            content

            for item in items key item.id {
                row(item)
            }

            if let footer {
                footer
            }
        }
    }
}

Card(title: "Your cart", items: items) {
    paragraph { "Review before checkout." }
}
.row { item =>
    CartRow(item: item)
}
.footer {
    button { "Checkout" }
        .on(click: checkout)
}
```

- The required unnamed slot is the trailing block. Other slots follow the named-or-chained rule.
- A scoped slot is a function that returns a `View`, so it is fully typed.
- `if let footer` unwraps an optional slot, using ordinary optional binding.
- **Slots are opaque.** A component can place its children but not inspect or modify them (there is no `React.Children.map`).
