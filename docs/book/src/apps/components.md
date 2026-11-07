# Components

A component is a reusable piece of UI. In K it is **a single declaration** whose body holds its inputs,
state, functions, view and styles: Vue's single-file component, without the XML sections.

```k
// src/counter/Counter.k
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
                .class(.label)

            button { "Add" }
                .on(click: increment)
        }
    }

    style {
        .label { fontSize: .lg; }
    }
}
```

- A component has exactly **one `view { }`** block and **at most one `style { }`** block.
- The order of members is free.
- A file may hold several declarations, but at most one is `public`, and it matches the file name.
  Small helper components can live next to the component that uses them.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/counter/Context.k
struct CartItem {
    let id: Int
    let name: String
}

component CartRow {
    @Prop let item: CartItem

    view {
        text { item.name }
    }
}

component SearchResults {
    @Prop let query: String

    view {
        text { "Results for {query}" }
    }
}

service CartService {
    @State var items: List<CartItem> = []
}

struct Note {
    let title: String
}

component NoteCard {
    @Prop let note: Note
    @Emits let pinToggled: Event<Note>

    view {
        button { note.title }
            .on(click: { pinToggled.emit(note) })
    }
}

component Dialog {
    @Prop let title: String
    @Slot let content: View
    @Emits let closed: Event<Void>

    view {
        heading(level: 2) { title }
        content
    }
}

component TemperatureField {
    @Binding var value: Double

    view {
        text { "{value}°F" }
    }
}

component Card {
    @Prop let title: String
    @Prop let items: List<CartItem>
    @Slot let content: View
    @Slot let footer: View? = nil
    @Slot let row: (CartItem) -> View

    view {
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
```
-->

## A component's public API

Annotated fields declare what a parent can pass in and what it can hear back:

| Annotation | Meaning |
|---|---|
| `@Prop` | One-way input from the parent. Read-only in the child. |
| `@Binding` | Two-way input. The child may write it, and the parent's state changes. |
| `@Emits` | An event the component fires: something that happened, not a value that changed. |
| `@Slot` | Content passed in by the parent. |
| `@Stylable` | A class hook the parent may style. See [Styles](styles.md#styling-child-components-stylable). |

Everything else, such as `@State`, plain fields and functions, is internal to the component.

## Required arguments are named; optional ones are chained

- A prop **without** a default is required, and is passed as a **named argument**.
- A prop **with** a default, or an optional prop (`T?`), is set through a **generated chained method**.

```k
component Stepper {
    @Binding var value: Int          // required → named argument
    @Prop let step: Int = 1          // has a default → .step()
    @Prop let max: Int? = nil        // optional → .max()
~
~    view {
~        text { "{value}" }
~    }
}

~component Order {
~    @State var quantity: Int = 1
~
~    view {
Stepper(value: bind quantity)
    .step(5)
    .max(99)
~    }
~}
```

- The compiler generates the chained methods; you never write a builder.
- Setting the same option twice is a compile error.
- Optional settings are **order-independent**.
- **Modifiers**, cross-cutting behaviors such as `.tooltip()`, `.animate()` and `.track()`, are chained
  too, but **apply in the order written**.
- A prop can't share a name with a registered modifier. That is a compile error at the prop declaration.
- **A `Bool` prop with a default is chained with its value:** `.dashed(true)`, `.disabled(isSubmitting)`.
  There are no zero-argument flag methods.
- **The attributes of built-in elements are not modifiers.** `.label`, `.placeholder`, `.type`,
  `.pressed`, `.expanded` and `.role` are generated per element, like any optional attribute, so they
  don't reserve names: `@Prop let label: String` is fine. A component that wants an accessible name
  declares its own `label` prop and applies it to an element inside.

This is the same rule that built-in elements follow: `image(source:description:)` takes its required
values as arguments and its options, like `.loading(.lazy)`, as chained calls.

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
    @Prop let price: Money

    view {
        Stepper(value: bind quantity)

        text { "Total: {price * quantity}" }
    }
}
```

The compiler checks both sides:

- A child writing to a `@Prop` is an error, with a quick fix to change it to `@Binding`.
- Passing `bind x` where `x` isn't `@State` or `@Binding` is an error. Literals and `@Derived` values
  can't be bound.
- Passing `quantity` without `bind` to a `@Binding` is an error, with a quick fix to add `bind`.

Bindings work on nested fields: `bind user.address.city`. For a binding that converts values, build one
with `Binding(get:set:)`:

```k
~component Thermostat {
~    @State var celsius: Double = 20.0
~
~    view {
TemperatureField(value: Binding(get: { celsius * 9 / 5 + 32 }, set: { f => celsius = (f - 32) * 5 / 9 }))
~    }
~}
```

**Why isn't two-way binding inferred from writes?** Because a component's public API would then change
with its implementation: one write added during a refactor would silently give the child write access to
the parent's state. Hidden two-way data flow is what sank AngularJS. `bind` at the call site makes it
visible in review.

## Events

`@Emits` declares an event, and `emit` fires it. Use events for things that happened (submitted, closed,
deleted), and bindings for state the parent shares.

A parent listens with the same chained `.on(…)` it uses for element events, naming the `@Emits` field:

```k
~import k.observe.Log
~
~component NotePage {
~    @Prop let note: Note
~    @State var isOpen: Bool = false
~
~    func togglePin(note: Note) {}
~
~    view {
Counter(label: "Votes")
    .on(changed: { value => Log.info("votes: {value}") })

NoteCard(note: note)
    .on(pinToggled: togglePin)

Dialog(title: "Delete note?") {
    paragraph { "This can't be undone." }
}
    .on(closed: { isOpen = false })
~    }
~}
```

- **The handler's type follows the event's payload.** For `Event<T>` it is a function taking a `T`: a
  lambda with one parameter, or a function passed by name. An event without a payload is declared
  `Event<Void>`, fired with `emit()`, and handled by a lambda without parameters.
- **One event per `.on(…)` call,** for elements and components alike. `.on(click: f, keyDown: g)` is a
  compile error with a fix that splits it into two calls.
- **Listening is optional.** An event nobody listens to does nothing, so events are never required
  arguments.
- **Unknown event names are compile errors,** with a hint listing the component's `@Emits` fields.
- `emit` can be called only from inside the declaring component. A parent can't fire a child's event.

### Handlers

- **Handlers may be `async`.** The element or component doesn't wait for them, and leaving the page
  cancels a handler that is still running.
- **A handler can't be declared `throws`.** Nothing waits for it, so nothing could catch its error. It
  handles every error inside, usually by recording it in state. Passing a throwing function to `.on(…)`
  is a compile error, with a quick fix that wraps its body in `do`/`catch`.
- A form's submit handler is usually wrapped by `form.handle(…)`, whose action may throw
  `ValidationError` ([Forms](forms.md)). The handler `form.handle` returns doesn't throw.

## Slots

A component accepts content through **typed `@Slot` fields**:

```k,standalone
~struct CartItem {
~    let id: Int
~}
~
component Card {
    @Prop let title: String
    @Prop let items: List<CartItem>
    @Slot let content: View                  // required and unnamed → the trailing block
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
```

The parent passes the main content as a trailing block and the other slots by the named-or-chained rule:

```k
~component CartPage {
~    @Prop let items: List<CartItem>
~
~    func checkout() {}
~
~    view {
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
~    }
~}
```

- The required unnamed slot is the trailing block.
- A **scoped slot** is a function that returns a `View`, so the data it receives is fully typed.
- `if let footer` unwraps an optional slot with ordinary optional binding.
- **Slots are opaque.** A component can place its children, but it can't inspect or modify them. There
  is nothing like `React.Children.map`.

## State inside a component

`@State`, `@Derived` and `@Effect` make a component reactive. They are covered in
[Reactivity](reactivity.md):

```k
component SearchBox {
    @State var query: String = ""
    @Derived let isSearching: Bool = query.isNotEmpty

    view {
        textField(value: bind query)
            .label("Search")

        if isSearching {
            SearchResults(query: query)
        }
    }
}
```

## Injecting services

Components that need shared state or remote data inject a [service](services.md):

```k
component CartBadge {
    @Inject let cart: CartService

    view {
        text { "{cart.items.count}" }
            .class(.badge)
    }

    style {
        .badge { fontWeight: bold; }
    }
}
```

Components may inject services, but **never protocols or repositories**. Remote calls always go through a
service.

## Loading data

A component can load its own async data with `@Load`, which makes it its own loading boundary, so a slow
sidebar doesn't hold up the page. See [Async data loading](async-data.md).
