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
- **Events are chained:** `.on(click: handler)`.
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
                .retry(load)
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
a `textField` binds a `String`, a `numberField` an `Int` or `Double`, a `checkbox` a `Bool`.

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
```

> **Proposal.** `checkbox(isChecked:)` and the accessibility modifiers below follow
> `docs/design/proposals/element-accessibility.md`.

```k
checkbox(isChecked: bind todo.isDone)
    .label("Done")
```

`.label("…")` renders a visible `<label>` connected to the field. See
[Forms and validation](forms.md).

## Events and modifiers

Events are chained with `.on(…)`. A handler is a function passed by name, or a lambda:

```k
button { "Save" }
    .on(click: save)

form { … }
    .on(submit: signup.handle(submit))

button { "−" }
    .on(click: { value -= step })
```

Async handlers are fine: `.on(click: checkout)` works when `checkout` is `async`.

Other attributes and behaviors are typed chained calls too:

| Modifier | Effect |
|---|---|
| `.class(.name)`, `.class([.a, .b when isB])` | Classes from the component's `style` block |
| `.type(.submit)`, `.type(.button)`, `.type(.reset)` | A button's type |
| `.disabled(Bool)` | Disables a control |
| `.placeholder("…")` | A field's placeholder text |
| `.label("…")` | A field's visible label; on any other element, its accessible name |
| `.pressed(Bool)` | `aria-pressed`, on buttons that toggle |
| `.expanded(Bool)` | `aria-expanded`, on buttons and links that open something |
| `.role(.alert)`, `.role(.status)` | Live regions that announce changes |

Each modifier takes a typed value, and is only allowed on elements where it makes sense. `.pressed` on a
`box` is a compile error, because only buttons can be pressed; `section { }.role(.navigation)` is a
compile error with the hint "use `nav`".

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

## Components from the standard library

The standard library's `k.ui` module builds on these elements:

- **Layout:** `Stack`, `Grid`, `Spacer`.
- **Accessible widgets:** `Dialog`, `Tabs`, `Menu`, `Popover`, `Tooltip`, `Toast`, `Combobox`,
  `DatePicker`, `DataTable`.
- **Icons:** `Icon`, with a bundled icon set.
- **`ErrorBoundary`**, which catches [panics](../language/errors.md#panics) in its subtree and renders a
  fallback.

They're capitalized because they're components, and they're imported like any other declaration:

```k
import k.ui.Dialog
```

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

> **Planned.** `k fmt`, which applies this layout automatically, is deferred.
