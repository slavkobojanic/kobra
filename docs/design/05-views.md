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
- **Events are chained:** `.on(click: handler)`.
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
                .retry(load)
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

## Formatting

The formatter (`k fmt`) enforces one canonical layout:

- **One blank line between sibling view nodes.** No blank line directly after `{` or before `}`.
- Chained calls stay attached to their node with no blank lines in between. With two or more chained calls, each goes on its own line.

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
