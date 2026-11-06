# Views and built-in elements

A component's `view { }` block describes its UI. It isn't HTML or XML: it's K code, written with the same
calls, trailing blocks and chained methods as everything else, and checked by the same type checker.

```k
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
            ErrorBanner(message: error.message)
                .on(retry: load)
        }
    }
}
```

A `for` in a view **requires a `key`**, which identifies each element across updates so that exactly the
right DOM nodes move, change or disappear.

## Built-in element names

HTML's abbreviated names are replaced by **full English names in camel case**. Each compiles to the
correct semantic HTML element. Writing an HTML name is a compile error with a hint, for example
"`a` → did you mean `link`?".

| HTML | K | Notes |
|---|---|---|
| `a` | `link(to: .page)` / `link(url: "https://…")` | Internal links are typed pages |
| `img` | `image(source:, description:)` | `alt` → `description`, required |
| `div` | `box` | |
| `span` | `text` | |
| `p` | `paragraph` | |
| `h1`–`h6` | `heading(level: n)` | |
| `ul` / `ol` / `li` | `list` / `numberedList` / `item` | |
| `br` / `hr` | `lineBreak` / `divider` | |
| `em` | `emphasis` | |
| `input` | `textField`, `passwordField`, `numberField`, `checkbox`, `dateField`, `fileField`, … | Split by type, so the bound value's type matches |
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
link(to: .product(id: product.id)) { product.name }

link(url: "https://example.com/terms") { "Terms of service" }
```

See [Pages and routing](routing.md).

### Images

`image` requires a text description, because an image without one is inaccessible:

```k
image(source: user.avatarUrl, description: "Profile photo of {user.name}")
    .class(.avatar)
    .loading(.lazy)
```

### Form fields

`input` is split into one element per kind of value, so a field's bound value always has the right type:
a `textField` binds a `String` (or a type such as `EmailAddress` that parses from text), a `numberField`
an `Int` or `Double`, a `checkbox` a `Bool`.

```k
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
```

Every input takes its bound value as its required argument. `checkbox(isChecked:)` binds a `Bool`, named
after the property it reflects. `.label("…")` renders a visible `<label>` connected to the field, and
tests find the field by it. See [Forms and validation](forms.md).

## Events, attributes and modifiers

Events are chained with `.on(…)`, one event per call. A handler is a function passed by name, or a lambda:

```k
button { "Save" }
    .on(click: save)

form { … }
    .on(submit: signup.handle(submit))

button { "−" }
    .on(click: { value -= step })
```

Async handlers are fine: `.on(click: checkout)` works when `checkout` is `async`. A handler can't throw;
see [Components](components.md#handlers).

### Attributes are per element

Optional attributes of built-in elements are typed chained calls, generated per element from the
element's definition. Using one on an element that doesn't define it is a compile error:

| Attribute | Compiles to | Defined on |
|---|---|---|
| `.class(.name)`, `.class([.a, .b when isB])` | scoped classes | every element |
| `.disabled(Bool)` | `disabled` | form controls and buttons |
| `.label("…")` | a visible `<label>` on form controls; `aria-label` elsewhere | form controls; landmarks and groups (`nav`, `section`, `form`, `list`, `numberedList`, `table`); elements given a `.role` |
| `.placeholder("…")` | `placeholder` | text inputs |
| `.type(.submit)`, `.type(.button)`, `.type(.reset)` | `type` | `button`. A non-submitting button inside a `form` needs `.type(.button)`. |
| `.pressed(Bool)` | `aria-pressed` | `button` |
| `.expanded(Bool)` | `aria-expanded` | `button`, `link` |
| `.role(.alert)`, `.role(.status)` | `role`; both are live regions | non-interactive elements |

```k
nav {
    for filter in filters key filter {
        button { filter.title }
            .pressed(filter == selection)
            .on(click: { selection = filter })
    }
}
.label("Filter todos")
```

- **On a landmark or group, `.label` is its accessible name.** The compiler warns when a second landmark
  of the same kind, such as a second `nav`, has no name.
- `.role` accepts only roles the element doesn't already have and that no semantic element expresses:
  `section { }.role(.navigation)` is a compile error with the hint "use `nav`".
- `.pressed` on a `box` is a compile error, because only buttons can be pressed.

### Buttons and links are named by their content

**`.label` isn't defined on `button` or `link`.** Their accessible name is their visible content, which
voice-control users say aloud. To add words for screen readers only, put them in the content with
`VisuallyHidden` from `k.ui`:

```k
button {
    "Delete"

    VisuallyHidden { " {todo.title}" }
}
.on(click: delete)
```

Its accessible name is "Delete Buy milk", and it still starts with the visible text, so a test can find
it with `button(named: "Delete Buy milk")`. On an element with visible text and a `.role`, a `.label` that
doesn't contain the visible text is a warning, for the same reason.

### Modifiers

**Modifiers** are cross-cutting behaviors from the standard library, such as `.tooltip(…)`,
`.animate(…)` and `.track(…)`. Unlike attributes, they apply in the order written, and a component prop
can't share a modifier's name.

## Components from the standard library

The standard library's `k.ui` module builds on these elements:

- **Layout:** `Stack`, `Grid`, `Spacer`.
- **Accessible widgets:** `Dialog`, `Tabs`, `Menu`, `Popover`, `Tooltip`, `Toast`, `Combobox`,
  `DatePicker`, `DataTable`.
- **Icons:** `Icon`, with a bundled icon set.
- **`VisuallyHidden`**, for text that only assistive technology reads.
- **`ErrorBoundary`**, which catches [panics](../language/errors.md#panics) in its subtree and renders a
  fallback.

They're capitalized because they're components, and they're imported like any other declaration:

```k
import k.ui.Dialog
```

### Charts

`k.charts` provides chart components built on SVG and themed by tokens. Like `image`, **every chart
requires a text description**, and it renders an accessible data table next to the graphic:

```k
import k.charts.LineChart
import k.charts.LineSeries

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
.yAxisTitle("°C")
.height(240px)
```

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
view {
    article {
        heading(level: 2) { title }

        content

        for item in items key item.id {
            row(item)
        }
    }
}
```

> **Planned.** `k fmt`, which applies this layout automatically, isn't in the first compiler releases.
