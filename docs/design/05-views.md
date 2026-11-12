# Views and Elements

## The view block

`view { }` describes a component's UI using a non-XML builder syntax.

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

## Rules

- **Lowercase names are built-in elements. Capitalized names are components,** from the stdlib or your own.
- **Built-in elements are type-checked.** Required attributes are named arguments (`image` requires `description`), and invalid values or misspelled attributes are compile errors.
- **Text is a string literal written directly inside a block.** No wrapper is needed.
- **Events are chained, one per call:** `.on(click: handler)`. See [04-components.md](04-components.md#events) for handler rules.
- **A `form` with `.on(submit:)` never navigates.** The browser's own form submission (a page load of the form's URL) is always prevented, before the handler runs and whether or not it throws, so the page and what was typed stay. There is no opt-out: a K form is handled by K, and leaving the page is a `link` or a navigation. A `form` without a `submit` handler keeps the browser's behaviour. (Decided after the real-browser runs found every `.on(submit:)` reloading the page.)
- **Classes are typed references** to the component's `style` block: `.class(.muted)`. See [06-styles.md](06-styles.md).

## Control flow

- `if` / `else` and `if let` for conditionals. There are no ternaries.
- `for item in items key item.id`: the `key` keyword is part of the loop syntax.
- `match` for exhaustive branching on enums (see [08-types.md](08-types.md)):

```k
view {
    match user {
        case .idle, .loading {
            Spinner()
        }

        case .loaded(let u) {
            ProfileCard(user: u)
        }

        case .failed(let error) {
            ErrorBanner(message: error.message)
                .on(retry: load)
        }
    }
}
```

## Built-in element names

HTML's abbreviated names are replaced with **full English names in camel case**. Each one compiles to the correct semantic HTML. Writing an HTML name is a compile error with a hint, for example: "`a` → did you mean `link`?"

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

These keep their HTML names: `button`, `form`, `label`, `nav`, `main`, `section`, `article`, `aside`, `header`, `footer`, `figure` (with `caption`), `video`, `audio`, `canvas`.

The stdlib provides layout components (`Stack`, `Grid`, `Spacer`) and accessible widgets (`Dialog`, `Tabs`, `Menu`) built on these elements.

## Inputs and accessibility attributes

### Inputs bind their value

Every input takes its bound value as the required argument, with `bind x` or a custom `Binding` ([04-components.md](04-components.md)):

```k
textField(value: bind signup.email)
    .label("Email")

checkbox(isChecked: bind todo.isDone)
    .label("Done")
```

- `checkbox(isChecked:)` binds a `Bool`, named after the property it reflects, with the `is` prefix the design uses for booleans.

### Attributes are per element

`.label`, `.placeholder`, `.type`, `.pressed`, `.expanded` and `.role` are **attributes of built-in elements**, generated per element from the element's typed definition like any optional attribute. They are not registered modifiers, so they don't reserve names for component props. Using one on an element that doesn't define it is a compile error.

| Attribute | Compiles to | Defined on |
|---|---|---|
| `.label("…")` | a visible `<label>` on form controls; `aria-label` elsewhere | form controls; landmarks and groups (`nav`, `section`, `form`, `list`, `numberedList`, `table`); elements given a `.role` |
| `.placeholder("…")` | `placeholder` | text inputs |
| `.autofocus(Bool)` | `autofocus`, and the focus when the control appears | form controls |
| `.type(.submit)`, `.type(.button)`, `.type(.reset)` | `type` | `button`. `.type(.button)` is needed on a non-submitting button inside a `form`. |
| `.pressed(Bool)` | `aria-pressed` | `button` |
| `.expanded(Bool)` | `aria-expanded` | `button`, `link` |
| `.role(.alert)`, `.role(.status)` | `role` (both are live regions) | non-interactive elements |

- **On a form control, `.label` is the visible label,** whatever its value: a literal, an interpolated string (`.label("New title for {todo.title}")`) or a computed `String` (`.label(todo.title)`). It renders as a `<label for>` before the control, and a computed one is a text binding that follows its value. A control that needs a name but no visible text puts `VisuallyHidden { … }` around a whole labelled group instead. Tests find the control by it: `textField(labeled:)`, `checkbox(labeled:)` ([21-testing.md](21-testing.md)).
- **`.autofocus(true)` on a form control moves the focus into it when it appears:** on the page's first render (the HTML `autofocus` attribute) and whenever an `if`, `match` or `for` block inserts it later, such as an edit form opened by an Edit button or a new-card form opened by "Add a card". Hydrating the server's markup doesn't move the focus a second time. Without it, inserting a control never moves the focus.
- **On a landmark or group, `.label` is its accessible name** (`nav { … }.label("Filter todos")`). The compiler warns when a second landmark of the same kind, such as a second `nav`, has no name.
- **`.label` is not defined on `button` or `link`.** Their accessible name is their visible content, which voice-control users say aloud (WCAG 2.5.3, label in name). To add words for screen readers only, put them in the content with `VisuallyHidden` from `k.ui`:

  ```k
  button {
      "Delete"

      VisuallyHidden { " {todo.title}" }
  }
  .on(click: delete)
  ```

  Its accessible name is "Delete Buy milk", and it still starts with the visible text.
- **On an element with visible text and a `.role`, a `.label` that doesn't contain the visible text is a warning,** for the same reason.
- `.role` accepts only roles the element doesn't already have and that no semantic element expresses. `section { }.role(.navigation)` is a compile error with the hint "use `nav`".
- `.pressed` on a `box` is a compile error: only buttons can be pressed.

## Formatting

The formatter (`k fmt`, [56-formatter.md](56-formatter.md)) enforces one canonical layout:

- **One blank line between sibling view nodes.** No blank line directly after `{` or before `}`.
- Chained calls stay attached to their node with no blank lines in between. With two or more chained calls, each goes on its own line.
- **A chain after a one-line node is indented one level; after a multi-line node it stays at the node's level,** below its closing `}` (`box {` … `}` then `.class(.panel)`). `let` constants in a view keep the blank lines the author gave them.

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
