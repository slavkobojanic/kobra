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
