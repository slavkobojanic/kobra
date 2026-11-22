# Styles and themes

Sources: `book/apps/styles.md` and `design/06-styles.md` and `07-themes-and-units.md`.

## `style` blocks

Each component, page and layout may have one `style { }`. It keeps CSS selectors and value syntax,
with one change: **names are camelCase**, both property names and keyword values (`justifyContent:
spaceBetween`, `borderLeft`). The kebab-case form is an error (K3201 for a property, K3214 for a
keyword). Declarations end with `;`, the one place in Kobra that has semicolons.

```k
style {
    .page {
        maxWidth: 40rem;
        margin: 0 auto;
        padding: .lg;

        @media (maxWidth: 600px) { padding: .sm; }
    }

    .muted { color: .muted; }
    .panel {
        maxHeight: 0;
        overflow: hidden;
        borderLeft: 4px solid .primary;

        &:hover { backgroundColor: .surfaceHover; }
    }
    .expanded { maxHeight: 400px; }
}
```

- **Selectors**: classes, `&` nesting, pseudo-classes and pseudo-elements, attribute selectors and
  `*`. **Element and `#id` selectors aren't allowed, and neither is `!important`.**
- At-rules are `@media`, `@container` and `@layer`, with camelCase features: `@media (maxWidth: 600px)`.
- **Values are type-checked** against each property's grammar. Theme tokens use implicit member syntax
  (`padding: .md`, `color: .muted`). Reactive values use braces (`borderLeft: 4px solid {accent};`),
  which compile to a CSS custom property updated through signals.
- Units are Kobra unit literals: `16px * 2` folds to `32px`, and `100px + 2rem` becomes `calc()`.

## Class references

- `.class(.muted)` refers to this declaration's own `.muted` class. A class that doesn't exist is an
  error (K3203), and an unused one is a warning (K3204).
- Several classes, some conditional: `.class([.panel, .expanded when isExpanded])`. Calling `.class`
  twice on one element is K3104.
- Classes are scoped (hashed), so never select them by name in tests or scripts. The escape hatch is
  `.class(global("btn-primary"))`.

## Styling a child: `@Stylable`

A component is closed to outside styling unless it declares `@Stylable` fields and applies them:

```k
component CartRow {
    @Prop let item: CartItem
    @Stylable let container: Classes

    view {
        box {
            text { item.name }

            text { item.price.formatted(as: .currency) }
                .class(.price)
        }
        .class([.row, container])
    }

    style {
        .row { display: flex; gap: .sm; }
        .price { color: .muted; }
    }
}
```

The parent passes classes from its own style block: `CartRow(item: item).container(.highlighted when
item.id == selectedId)`. There's no `::part()` and no `:deep()`.

## The theme

There's one `theme` per app (it's usually `src/theme/AppTheme.k`, and `k new` writes one). Each token
becomes a static member of its type. Using a token that nothing declares is a compile error.

```k
public theme AppTheme {
    color primary: Color = #2563EB
    color surface: Color = #FFFFFF
    color surfaceHover: Color = #F3F4F6
    color muted: Color = #6B7280

    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px

    radius md: Radius = 8px

    fontSize sm: FontSize = 14px
    fontSize lg: FontSize = 20px

    font body: Font = Font(family: "system-ui", size: 16px)

    variant dark matches .prefersDark {
        surface = #111827
        surfaceHover = #1F2937
    }
}
```

- The categories are distinct types (`Spacing`, `Radius`, `FontSize`, `Color`, `Font`, …), so a
  `Color` token for `padding` is an error.
- **Declaring a token creates it, and assigning one overrides it.** `variant dark { surface = … }`
  overrides tokens and can't add new ones. A library's tokens (`chartGrid`, from `k.charts`) are
  overridden by assignment, and re-declaring one is K3302.
- `variant dark matches .prefersDark` follows the OS through CSS, as does `.prefersContrast`.
- Base styles come from the theme: the body uses `font body`, and its text and background use the
  `text` and `background` color tokens (or their fallbacks).
- Kobra loads no web fonts. A family is used where it's installed, with a generic fallback stack.

## Switching variants at runtime

The generated `ThemeVariant` enum has `.base` plus one case per variant. The client-only `Theme`
service (`import k.ui.Theme`) switches between them:

```k
func toggleTheme() {
    theme.apply(if theme.current == .dark { .base } else { .dark })
}
```

`theme.current` is reactive. `theme.apply(_:)` remembers the choice in a cookie, so server rendering
paints it right, and `theme.followSystem()` forgets the choice.

## Colors

`.primary.opacity(0.8)`, `.primary.mix(.white, 20%)`, `Color.hex("#3B82F6")` and
`Color.rgb(59, 130, 246)` are folded at compile time where possible.
