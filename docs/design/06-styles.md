# Styles

## The style block

Each component has at most one `style { }` block. It uses **CSS selectors and value syntax, with camel-case property names** mapped 1:1 to CSS (`borderLeft` → `border-left`).

```k
component CartSummary {
    @Prop let accent: Color = .primary
    @State var isExpanded: Bool = false

    view {
        section {
            paragraph { "Nothing here yet." }
                .class([.muted, .small])

            box { ... }
                .class([.panel, .expanded when isExpanded])
        }
        .class(.summary)
    }

    style {
        .summary {
            padding: .md;
            borderLeft: 4px solid {accent};
            backgroundColor: .surface;

            &:hover { backgroundColor: .surfaceHover; }

            @media (maxWidth: 600px) { padding: .sm; }
        }

        .muted    { color: .muted; }
        .small    { fontSize: .sm; }
        .panel    { maxHeight: 0; overflow: hidden; }
        .expanded { maxHeight: 400px; }
    }
}
```

## Rules

- **Property values are type-checked** against each property's CSS grammar. `borderLeft: 4px blue solid potato` is a compile error.
- **Selectors, pseudo-classes, `@media`, `@container`, `@layer` and nesting are kept from CSS.** Only property names change.
- **Theme tokens and static values use implicit member syntax:** `padding: .md` resolves against the type the property expects. See [07-themes-and-units.md](07-themes-and-units.md).
- **`{ }` is used only for reactive values** (props and state). `{accent}` compiles to a CSS custom property, updated through the signal system. No inline `style=` strings are needed.
- **Position decides meaning:** at the rule level, `.summary` is a class selector; after `property:`, `.md` is a value.

## Class references

- `.class(.muted)` refers to a class in this component's `style` block.
- **Referencing a class that doesn't exist is a compile error. A defined class that is never used is a warning.**
- **Conditional classes use `when`:** `.class([.panel, .expanded when isExpanded])`.
- Setting `.class()` twice is a compile error. Use a list.

## Scoping

- **Styles are scoped by default.** The compiler hashes class names, so `.panel` in one component can't collide with `.panel` in another.
- **Global CSS requires an explicit escape hatch:** `.class(global("btn-primary"))`.

## Styling child components: `@Stylable`

**A component is closed to outside styling unless it opts in.** It opts in by declaring `@Stylable` props and choosing exactly which elements receive them:

```k
component CartRow {
    @Prop let item: CartItem
    @Stylable let container: Classes
    @Stylable let title: Classes

    view {
        box {
            heading(level: 3) { item.name }
                .class([.title, title])

            paragraph { item.price.formatted(as: .currency) }
                .class(.price)              // not stylable from outside
        }
        .class([.row, container])
    }
}

// Parent
CartRow(item: item)
    .container(.highlighted when item.id == selectedId)
    .title(.strikethrough when item.isRemoved)
```

- `@Stylable` props are optional props, so they become chained methods automatically.
- The parent passes classes from **its own** `style` block, still typed and scoped.
- A `@Stylable` prop that is never applied to an element is a compile error.
- **There is no `::part()` and no `:deep()`.** A component without `@Stylable` props can't be styled from outside. The parent can still lay it out by wrapping it or placing it in a grid.
