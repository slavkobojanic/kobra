# Views and built-in elements

A component's `view { }` block describes its UI. It isn't HTML or XML: it's Kobra code, written with the same
calls, trailing blocks and chained methods as everything else, and checked by the same type checker.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/shop/Context.k
import k.auth.Session

struct CartItem {
    let id: Int
    var quantity: Int
}

component CartRow {
    @Prop let item: CartItem
    @Binding var quantity: Int

    view {
        text { "{quantity}" }
    }
}

struct Product {
    let id: Int
    let name: String
}

component ProductCard {
    @Prop let product: Product

    view {
        text { product.name }
    }
}

struct Profile implements Codable {
    var name: String
    var age: Int
    var country: Country
}

enum Country implements Codable {
    case norway
    case sweden

    var name: String {
        match self {
            case .norway { "Norway" }
            case .sweden { "Sweden" }
        }
    }
}

enum ProfileError implements Error, Codable {
    case unavailable

    var message: String { "The profile couldn't be loaded." }
}

component Spinner {
    view {
        text { "Loading…" }
    }
}

component ProfileCard {
    @Prop let profile: Profile

    view {
        text { profile.name }
    }
}

component ErrorBanner {
    @Prop let message: String
    @Emits let retry: Event<Void>

    view {
        text { message }
    }
}

struct User {
    let name: String
    let avatarUrl: Url
}

struct Todo {
    let title: String
    var isDone: Bool
}

enum TodoFilter {
    case all
    case open
    case done

    var title: String {
        match self {
            case .all { "All" }
            case .open { "Open" }
            case .done { "Done" }
        }
    }
}

page SignIn at "/sign-in" {
    view {
        text { "Sign in" }
    }
}

page ProductPage at "/product/{id}" {
    @Param let id: Int

    view {
        text { "Product {id}" }
    }
}
```
-->

```k
~component Checkout {
~    @State var items: List<CartItem> = []
~    @State var isSubmitting: Bool = false
~    let total: Money = 0.usd
~
~    func checkout() {}
view {
    section {
        heading(level: 2) { "Your cart" }

        for item in items key item.id {
            CartRow(item: item, quantity: bind item.quantity)
        }

        if items.isEmpty {
            paragraph { "Nothing here yet." }
                .class(.muted)
        } else {
            button { "Pay {total.formatted(as: .currency)}" }
                .type(.submit)
                .disabled(isSubmitting)
                .on(click: checkout)
        }
    }
    .class(.checkout)
}
~    style {
~        .checkout { padding: .md; }
~        .muted { color: .muted; }
~    }
~}
```

## The rules

- **Lowercase names are built-in elements. Capitalized names are components**, from the standard
  library or your own code.
- **Built-in elements are type-checked.** Required attributes are named arguments (`image` requires a
  `description`), and invalid values or misspelled attributes are compile errors.
- **Text is a string literal written directly inside a block.** No wrapper is needed.
- **Events are chained, one per call:** `.on(click: handler)`.
- **Classes are typed references** to the component's `style` block: `.class(.muted)`. See
  [Styles and themes](styles.md).

Every statement in a view is a view node:

| In a view | Means |
|---|---|
| `"Total: {total}"` | a text node |
| `paragraph { … }`, `heading(level: 2) { title }` | a built-in element with children |
| `CartRow(item: item)` | a component |
| `content`, `row(item)` | a slot, or a call to a scoped slot |
| `.class(…)`, `.on(click: f)`, `.disabled(x)` | attributes, events and modifiers, chained onto the node above |
| `if` / `else`, `if let` | conditional content |
| `for item in items key item.id { … }` | a keyed list |
| `match x { case … }` | branching on an enum |
| `let x = …` | a local constant |

## Control flow in views

Views use the ordinary [control flow](../language/control-flow.md) constructs. There are no ternaries
and no special template directives.

```k
~import k.auth.Session
~
~component Dashboard {
~    @Inject let session: Session
~    @Prop let products: List<Product>
~    @State var state: LoadState<Profile> = .idle
~
~    func load() {}
view {
    if let user = session.user {
        paragraph { "Signed in as {user.id}" }
    } else {
        link(to: .signIn) { "Sign in" }
    }

    for product in products key product.id {
        ProductCard(product: product)
    }

    match state {
        case .idle, .loading {
            Spinner()
        }

        case .loaded(let profile) {
            ProfileCard(profile: profile)
        }

        case .failed(let error) {
            ErrorBanner(message: "Couldn't load the profile: {error}")
                .on(retry: load)
        }
    }
}
~}
```

A `for` in a view **requires a `key`**, which identifies each element across updates so that exactly the
right DOM nodes move, change or disappear.

## Built-in element names

HTML's abbreviated names are replaced by **full English names in camel case**. Each compiles to the
correct semantic HTML element. Writing an HTML name is a compile error with a hint, for example
"`a` → did you mean `link`?".

| HTML | Kobra | Notes |
|---|---|---|
| `a` | `link(to: .page)` / `link(url: Url("https://…"))` | Internal links are typed pages |
| `img` | `image(source:, description:)` | `alt` → `description`, required |
| `div` | `box` | |
| `span` | `text` | |
| `p` | `paragraph` | |
| `h1`–`h6` | `heading(level: n)` | |
| `ul` / `ol` / `li` | `list` / `numberedList` / `item` | |
| `br` / `hr` | `lineBreak` / `divider` | |
| `em` | `emphasis` | |
| `input` | `textField`, `passwordField`, `numberField`, `checkbox`, `dateField` | Split by type, so the bound value's type matches. Files are picked with the injected `Files` ([Importing and exporting files](files.md)) |
| `select` / `option` | `dropdown(selection: bind x)` / `option` | |
| `textarea` | `textArea` | |
| `table` / `tr` / `th` / `td` | `table` / `row` / `headerCell` / `cell` | |
| `pre` + `code` | `codeBlock` / `code` | |
| `dl` / `dt` / `dd` | `definitionList` / `term` / `definition` | |
| `sup` / `sub` | `superscript` / `subscript` | |

These keep their HTML names, because they are already full words: `button`, `form`, `label`, `nav`,
`main`, `section`, `article`, `aside`, `header`, `footer`, `figure` (with `caption`), `video`, `audio`
and `canvas`.

### Links

Internal links point at **typed pages**, never at URL strings. The compiler generates a `Page` enum with
a case for every page, so a link to a page that doesn't exist, or with missing parameters, is a compile
error:

```k
~component ProductLinks {
~    @Prop let product: Product
~
~    view {
link(to: .productPage(id: product.id)) { product.name }

link(url: Url("https://example.com/terms")) { "Terms of service" }
~    }
~}
```

See [Pages and routing](routing.md).

### Images

`image` requires a text description, because an image without one is inaccessible:

```k
~component Avatar {
~    @Prop let user: User
~
~    view {
image(source: user.avatarUrl, description: "Profile photo of {user.name}")
    .class(.avatar)
    .loading(.lazy)
~    }
~
~    style {
~        .avatar { borderRadius: .lg; }
~    }
~}
```

### Form fields

`input` is split into one element per kind of value, so a field's bound value always has the right type:
a `textField` binds a `String` (or a type such as `EmailAddress` that parses from text), a `numberField`
an `Int` (read decimals from a `textField` with `Double.parse`), a `checkbox` a `Bool`.

```k
~component ProfileForm {
~    @State var profile: Profile = Profile(name: "", age: 0, country: .norway)
~    @State var todo: Todo = Todo(title: "", isDone: false)
~    let countries: List<Country> = [.norway, .sweden]
~
~    view {
textField(value: bind profile.name)
    .label("Name")

numberField(value: bind profile.age)
    .label("Age")

dropdown(selection: bind profile.country) {
    for country in countries key country {
        option(value: country) { country.name }
    }
}
.label("Country")

checkbox(isChecked: bind todo.isDone)
    .label("Done")
~    }
~}
```

Every input takes its bound value as its required argument. `checkbox(isChecked:)` binds a `Bool`, named
after the property it reflects. `.label("…")` renders a visible `<label>` connected to the field, and
tests find the field by it. See [Forms and validation](forms.md).

## Events, attributes and modifiers

Events are chained with `.on(…)`, one event per call. A handler is a function passed by name, or a lambda:

```k
~component Buttons {
~    @Binding var value: Int
~    @Prop let step: Int = 1
~
~    func save() {}
~
~    view {
button { "Save" }
    .on(click: save)

form {
    // …
}
    .on(submit: save)

button { "−" }
    .on(click: { value -= step })
~    }
~}
```

**A `form` with `.on(submit:)` never navigates.** The browser's own submission, a page load, is always
prevented, before the handler runs and even when it fails, so the page and what was typed stay.

Async handlers are fine: `.on(click: checkout)` works when `checkout` is `async`. A handler can't throw;
see [Components](components.md#handlers).

### Attributes are per element

Optional attributes of built-in elements are typed chained calls, generated per element from the
element's definition. Using one on an element that doesn't define it is a compile error:

| Attribute | Compiles to | Defined on |
|---|---|---|
| `.class(.name)`, `.class([.a, .b when isB])` | scoped classes | every element |
| `.disabled(Bool)` | `disabled` | form controls and buttons |
| `.label("…")` | a visible `<label>` on form controls, literal or computed (`.label(todo.title)`); `aria-label` elsewhere | form controls; landmarks and groups (`nav`, `section`, `form`, `list`, `numberedList`, `table`); elements given a `.role` |
| `.placeholder("…")` | `placeholder` | text inputs |
| `.autofocus(Bool)` | `autofocus`; the focus moves in when the control appears, such as an edit form an `if` opens | form controls |
| `.type(.submit)`, `.type(.button)`, `.type(.reset)` | `type` | `button`. A non-submitting button inside a `form` needs `.type(.button)`. |
| `.pressed(Bool)` | `aria-pressed` | `button` |
| `.expanded(Bool)` | `aria-expanded` | `button`, `link` |
| `.role(.alert)`, `.role(.status)` | `role`; both are live regions | non-interactive elements |
| `.ref(field)` | nothing in the markup; names the element for an `ElementRef` field ([below](#focus-and-scrolling)) | every element |

```k
~component FilterBar {
~    @Binding var selection: TodoFilter
~    let filters: List<TodoFilter> = [.all, .open, .done]
~
~    view {
nav {
    for filter in filters key filter {
        button { filter.title }
            .pressed(filter == selection)
            .on(click: { selection = filter })
    }
}
.label("Filter todos")
~    }
~}
```

- **On a landmark or group, `.label` is its accessible name.** Give each of two landmarks of the same
  kind, such as two `nav`s, a name. A warning for a missing one is planned (**Planned** a11y-lints).
- `.role` accepts only roles the element doesn't already have and that no semantic element expresses:
  `section { }.role(.navigation)` is a compile error with the hint "use `nav`".
- `.pressed` on a `box` is a compile error, because only buttons can be pressed.

### Buttons and links are named by their content

**`.label` isn't defined on `button` or `link`.** Their accessible name is their visible content, which
voice-control users say aloud. To add words for screen readers only, put them in the content with
`VisuallyHidden` from `k.ui`:

```k
~import k.ui.VisuallyHidden
~
~component DeleteButton {
~    @Prop let todo: Todo
~
~    func delete() {}
~
~    view {
button {
    "Delete"

    VisuallyHidden { " {todo.title}" }
}
.on(click: delete)
~    }
~}
```

Its accessible name is "Delete Buy milk", and it still starts with the visible text, so a test can find
it with `button(named: "Delete Buy milk")`. On an element with visible text and a `.role`, a `.label`
should contain the visible text, for the same reason; the warning for one that doesn't is planned too.

### Focus and scrolling

A component names one of its own elements with an `ElementRef` field from `k.ui` and `.ref(…)`, and its
handlers move the focus there or scroll to it. No ids or selectors are involved, and the compiler checks
the names:

```k
~import k.ui.ElementRef
~
~component NewNote {
~    @State var text: String = ""
~    @State var notes: List<String> = []
~
let field = ElementRef()

func add() {
    notes = notes + [text]
    text = ""
    // Back to typing, even when the Add button was clicked.
    field.focus()
}

view {
    form {
        textField(value: bind text)
            .label("Note")
            .ref(field)

        button { "Add" }
            .type(.submit)
    }
    .on(submit: add)
}
~}
```

- **`focus(selectingText:)`, `blur()` and `scrollIntoView(alignment:animated:)`** run once the view has
  caught up, at the end of the handler or effect that called them, so a handler can open an editor and
  focus its field together. While the element isn't shown they do nothing. A heading or section takes the
  focus from code (it gets `tabindex="-1"`) without becoming a Tab stop, and `animated` scrolling jumps
  instead when the user prefers reduced motion.
- **A reference is a `let` field of the component, page or layout, made with `ElementRef()`,** and
  `.ref(…)` attaches it to exactly one element of that declaration's own view, outside `for` bodies and
  scoped slots (`K3113`, `K3114`; an unattached one is the `K3115` warning). A row that needs one becomes a
  component holding its own.
- **Only handlers, effects and `init()` call the operations**; a view, a `@Derived` or another field
  initializer can't (`K3116`), so nothing runs on the server and rendering never moves the focus.
- **A failed `@Form` submit focuses the first invalid field by itself** ([Forms](forms.md#accessible-by-default)).
  Where the focus goes after adding an item or closing an editor is the app's choice, made with a
  reference: the todos example returns it to the new-todo field, kanban to the card's Edit button.

- **A navigation moves the focus and the scroll by itself.** When a link or `Router.navigate` shows
  another page, the router focuses that page's first level-1 heading (it gets `tabindex="-1"`), so a
  screen reader announces where the user landed, and the page starts at the top, or at the element of
  the URL's `#fragment`. Back and Forward put each page where it was scrolled when it was left, and a
  reload keeps the position. The same page with other parameters (a filter in the query) is updated in
  place and keeps both, so typing into a search field that updates the URL never loses the focus.

### Modifiers

> **Planned** (modifiers). **Modifiers** are cross-cutting behaviors from the standard library, such as
> `.tooltip(…)`, `.animate(…)` and `.track(…)`. Unlike attributes, they will apply in the order written,
> and a component prop won't be able to share a modifier's name. None is built yet.

## Components from the standard library

The standard library's `k.ui` module builds on these elements:

- **Layout:** `Stack`, `Grid`, `Spacer`.
- **Accessible widgets** (**Planned** ui-widgets): the design adds `Dialog`, `Tabs`, `Menu`, `Popover`, `Tooltip`,
  `Toast`, `Combobox`, `DatePicker` and `DataTable`. None of them is built yet.
- **Icons:** `Icon`, with a bundled icon set.
- **`VisuallyHidden`**, for text that only assistive technology reads.
- **`ErrorBoundary`**, which catches [panics](../language/errors.md#panics) in its subtree and renders a
  fallback.

They're capitalized because they're components, and they're imported like any other declaration:

```k
import k.ui.VisuallyHidden
```

### Charts

`k.charts` provides chart components built on SVG and themed by tokens. Like `image`, **every chart
requires a text description**, and it renders an accessible data table next to the graphic:

```k
// examples/weather/src/components/TemperatureChart.k
LineChart(
    series: [
        LineSeries(name: "Temperature", points: temperature)
            .color(.chartTemperature),
        LineSeries(name: "Feels like", points: feelsLike)
            .color(.chartFeelsLike)
            .dashed(true)
    ],
    description: "Temperature and feels-like temperature for the next 24 hours"
)
.yAxisTitle(units.temperatureSymbol)
.timeZone(timeZone)
.height(240px)
```

The weather example's `TemperatureChart` imports `k.charts.LineChart` and `k.charts.LineSeries`, and
builds `temperature` and `feelsLike` as lists of `ChartPoint`s. [Charts](charts.md) covers them.

- The series are data, not slots, because a chart has to read the values to compute its scales.
- `ChartPoint(x:y:)` takes an `Int`, `Double` or `Date` for `x`. `Date` axes get time ticks in the
  chart's `timeZone`, which defaults to the browser's.
- Charts are themed through library tokens such as `chartGrid` and `chartAxis`, so they follow dark mode
  with no chart code. See [Styles and themes](styles.md#library-tokens).

## Formatting

There is one canonical layout for views:

- **One blank line between sibling view nodes.** No blank line directly after `{` or before `}`.
- Chained calls stay attached to their node, with no blank lines in between. With two or more chained
  calls, each goes on its own line.

```k
~component Card {
~    @Prop let title: String
~    @Prop let items: List<CartItem>
~    @Slot let content: View
~    @Slot let row: (CartItem) -> View
view {
    article {
        heading(level: 2) { title }

        content

        for item in items key item.id {
            row(item)
        }
    }
}
~}
```

`k fmt` applies this layout, and so does your editor when it formats a file.
