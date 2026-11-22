# Components, views and reactivity

Sources: `book/apps/components.md`, `views.md`, `reactivity.md` and `async-data.md`, and
`design/03-reactivity.md`, `04-components.md` and `05-views.md`.

## A component is one declaration

It holds inputs, state, functions, exactly one `view { }`, at most one `style { }`, and optionally
`loading { }` and `failed { }`. Members can come in any order.

| Annotation | Keyword | Meaning |
|---|---|---|
| `@Prop` | `let` | one-way input. Without a default it's a required **named argument**. With a default or `T?`, it's set with a **chained call** |
| `@Binding` | `var` | two-way input. The parent passes `bind x` |
| `@Emits` | `let` | `Event<T>`, fired with `name.emit(value)`. The parent listens with `.on(name: handler)` |
| `@Slot` | `let` | `View` (the required unnamed slot is the trailing block), `View?` (optional, chained), or `(T) -> View` (a scoped slot) |
| `@Stylable` | `let` | `Classes` the parent may style |
| `@State` | `var` | reactive mutable state, private to the component |
| `@Derived` | `let` | computed from reactive values, lazily, with no dependency arrays |
| `@Effect` | func | re-runs when what it read changes. `@Effect(on: [a, b])` names the dependencies, and `@Effect(on: [])` runs once after mount |
| `@Load` | `let` | async data the view waits for. Needs a `loading { }` block |
| `@Inject` | `let` | a **service**. Components never inject protocols (K4005) or repositories |

A plain `var` field is ordinary, non-reactive data.

### Props, bindings, events

```k
component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1
    @Prop let max: Int? = nil

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

component Order {
    @Prop let price: Money
    @State var quantity: Int = 1

    view {
        Stepper(value: bind quantity)
            .step(5)
            .max(99)

        text { "Total: {price * quantity}" }

        Counter(label: "Votes")
            .on(changed: { value => Log.info("votes: {value}") })
    }
}
```

- Writing to a `@Prop` is K3014, and its fix is `@Binding`. Passing a value to a `@Binding` without
  `bind` is K3016.
- **Only `@State`, `@Binding` and `@Form` values, and paths into them, can be bound (K3015).** A
  `@Derived` value can't be bound, and neither can the loop variable of a `for` over one. Loop over
  the `@State` list itself (`for todo in todos key todo.id { checkbox(isChecked: bind todo.isDone) }`).
- Nested paths bind fine: `bind user.address.city`. For a converting binding, use
  `Binding(get: { … }, set: { value => … })`.
- A `Bool` option is chained with its value (`.disabled(isSubmitting)`). There are no zero-argument
  flags.
- Each `.on(…)` call takes one event. An `Event<Void>` is fired with `emit()` and handled by a lambda
  with no parameters.
- **Handlers may be `async` but never `throws`** (K3017). Async work belongs to the component that
  started it: it's cancelled when that component unmounts, and no code after an `await` runs once the
  component is gone.

### Slots

```k
public component Card {
    @Prop let title: String
    @Prop let items: List<CartItem>
    @Slot let content: View
    @Slot let footer: View? = nil
    @Slot let row: (CartItem) -> View

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

```k
Card(title: "Your cart", items: items) {
    paragraph { "Review before checkout." }
}
.row { item =>
    CartRow(item: item)
        .container(.highlighted when item.id == selectedId)
}
.footer {
    button { "Checkout" }
        .disabled(isSubmitting)
        .on(click: checkout)
}
```

Slots are opaque: a component places its children, but can't inspect them.

## Reactivity

- Signals are fine-grained and compiled, with no virtual DOM. Writing `user.address.city = "Bergen"`
  updates only the nodes that read that path, because structs are values.
- Writing to a `@Derived`, or writing `@State` inside a `@Derived` computation, is an error.
- **Effects run only in the browser**, after mount or hydration, never during server rendering. A new
  run cancels an async run still in flight, which makes debouncing trivial:

```k
@Effect
func search() async {
    let text = query

    // A new keystroke cancels this run while it sleeps.
    await clock.sleep(300ms)

    do {
        results = .loaded(try await places.search(text))
    } catch let error {
        results = .failed(error)
    }
}
```

- `onCleanup { … }` runs before the next run and on unmount, and `untracked { … }` reads without
  subscribing. Cancel timers in cleanup:

```k
@Effect(on: [filter])
func tick() {
    let timer = clock.every(30.seconds) { ticks += 1 }

    onCleanup { timer.cancel() }
}
```

- Services hold shared state with the same annotations. A component reading `cart.items` updates when
  the service changes it. Only the service's own methods can write its fields.

## Async data: `@Load`, `loading`, `failed`

```k
public component CurrentTemperature {
    @Inject let forecasts: ForecastService
    @Inject let clock: Clock

    @Load let forecast: Forecast = try await forecasts.current(latitude: 59.9, longitude: 10.7)

    @Effect(on: [])
    func refreshPeriodically() {
        let timer = clock.every(10.minutes) { reload() }

        onCleanup { timer.cancel() }
    }

    view {
        paragraph { "{forecast.temperature}°" }
    }

    loading {
        paragraph { "Loading…" }
    }

    failed(let error) {
        match error {
            case .offline, .timeout {
                paragraph { "You're offline." }
            }

            case .upstream, .invalidResponse {
                paragraph { "The forecast is unavailable." }
            }
        }

        button { "Try again" }
            .on(click: reload)
    }
}
```

- `view` renders once every `@Load` has resolved, so inside it the values are plain values. Loads
  that read other loads wait for them, and independent loads run in parallel.
- `loading` is required. `failed` is optional (without it the error goes to the nearest
  `ErrorBoundary`), and receives the loads' typed error.
- `reload()` is generated. It keeps showing the old values until the new ones arrive, and declaring
  your own `reload` is an error. `@Load(keepOnReload: true)` skips a value whose inputs haven't
  changed.
- With server rendering, `@Load` types and their errors must be `Codable`.
- Data fetched by a user action belongs in `@State var results: LoadState<T> = .idle`, which you
  `match` in the view (`case .idle, .loading`, `case .loaded(let found)`, `case .failed`), set by an
  effect or handler like the search above.
- `@Rendering(.client)` on a component renders it only in the browser. Its `loading` block is what the
  server renders, and it may read only `@Prop`s (K4028).

## Views

Every statement in a view is a node: a string literal is text, lowercase names are **built-in
elements**, capitalized names are **components**, and `if`, `if let`, `match`, `for … key` and `let`
all work. Attributes and events are chained calls on the node above.

| HTML | Kobra |
|---|---|
| `div`, `span`, `p` | `box`, `text`, `paragraph` (`div` is K3101) |
| `h1`–`h6` | `heading(level: n) { … }` |
| `a` | `link(to: .page(…)) { … }` for typed pages, `link(url: Url("https://…")) { … }` for outside links |
| `img` | `image(source: url, description: "…")`, where the description is required |
| `ul`/`ol`/`li` | `list`, `numberedList`, `item` |
| `input` | `textField(value:)`, `passwordField(value:)`, `numberField(value:)` (an `Int`), `checkbox(isChecked:)`, `dateField` |
| `select`/`option` | `dropdown(selection: bind x) { option(value: v) { … } }` |
| `textarea` | `textArea(value:)` |
| `table`/`tr`/`th`/`td` | `table`, `row`, `headerCell`, `cell` |
| `br`/`hr`/`em`/`pre` | `lineBreak`, `divider`, `emphasis`, `codeBlock`/`code` |
| unchanged | `button form label nav main section article aside header footer figure caption video audio canvas` |

```k
nav {
    for option in filters key option {
        button { option.title }
            .pressed(option == filter)
            .on(click: { filter = option })
    }
}
.label("Filter todos")

form {
    textField(value: bind text)
        .label("New todo")
        .placeholder("Buy milk")
        .ref(field)

    button { "Add" }
        .type(.submit)
}
.on(submit: add)
```

- Attributes are per element: `.class(…)` (only once per element, K3104), `.disabled(Bool)`,
  `.label("…")`, `.placeholder`, `.autofocus(Bool)`, `.type(.submit / .button / .reset)`,
  `.pressed(Bool)`, `.expanded(Bool)`, `.role(.alert / .status)` and `.ref(field)`.
- **`button` and `link` have no `.label`.** Their content is their accessible name. Add screen-reader
  text with `VisuallyHidden { " {todo.title}" }` from `k.ui`.
- A non-submitting button inside a `form` needs `.type(.button)`. A `form` with `.on(submit:)` never
  navigates.
- Focus: `let field = ElementRef()` (from `k.ui`), `.ref(field)`, and `field.focus()` from a handler,
  effect or `init`, never from the view.
- `k.ui` has `Stack`, `Grid`, `Spacer`, `Icon`, `VisuallyHidden`, `ErrorBoundary`, `Router` and
  `Theme`. `Dialog`, `Tabs`, `Menu` and the other widgets are **Planned**, so don't use them.
- Layout: one blank line between sibling nodes. With two or more chained calls, put each on its own
  line. `k fmt` enforces this.
