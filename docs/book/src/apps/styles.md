# Styles and themes

Each component, page and layout can have one `style { }` block. It keeps **CSS selectors and value
syntax**, and changes one thing: **property names are camelCase**, mapped one-to-one to CSS
(`borderLeft` → `border-left`). The compiler type-checks it, scopes it, and connects it to the theme.

```k
component CartSummary {
    @Prop let accent: Color = .primary
    @State var isExpanded: Bool = false

    view {
        section {
            paragraph { "Nothing here yet." }
                .class([.muted, .small])

            box { … }
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

## What stays CSS

- **Selectors**: classes, `&` nesting, pseudo-classes such as `:hover` and `:nth-child(2n + 1)`,
  pseudo-elements, attribute selectors and `*`.
- **At-rules**: `@media`, `@container` and `@layer`. Media feature names are camelCase like properties:
  `@media (maxWidth: 600px)`.
- **Values**: lengths, keywords such as `solid` and `hidden`, functions, and commas between values.
- Comments use `//` and `/* */`, as in K code.

Declarations end with `;`, as in CSS. This is the one place in K where semicolons appear.

What isn't kept: **element selectors and `#id` selectors.** Styles select the classes you define, never
`box` or `button`, and `!important` is a compile error.

## Values are type-checked

**Property values are checked against each property's CSS grammar.** `borderLeft: 4px blue solid potato`
is a compile error, not a silently ignored declaration.

Two kinds of K value appear in styles:

- **Theme tokens and static values use implicit member syntax:** `padding: .md` resolves against the type
  `padding` expects, so it means `Spacing.md`. They never need braces.
- **Reactive values use `{ }`:** `{accent}` reads a prop or state. It compiles to a CSS custom property
  that is updated through the signal system, so there is no need for inline `style=` strings, and no
  re-render when it changes.

Position decides meaning: at the start of a rule, `.summary` is a class selector; after `property:`,
`.md` is a value.

## Class references

```k
paragraph { "Nothing here yet." }
    .class(.muted)
```

- `.class(.muted)` refers to the `.muted` class in this component's own `style` block.
- **Referencing a class that doesn't exist is a compile error. A class that is defined but never used is
  a warning.**
- **Conditional classes use `when`:** `.class([.panel, .expanded when isExpanded])`.
- Several classes go in one list. Calling `.class()` twice on one element is a compile error.

## Scoping

**Styles are scoped by default.** The compiler hashes class names, so `.panel` in one component can't
collide with `.panel` in another, and no style leaks out of its component.

Global CSS is an explicit escape hatch:

```k
box { … }
    .class(global("btn-primary"))
```

## Styling child components: `@Stylable`

**A component is closed to outside styling unless it opts in.** It opts in by declaring `@Stylable`
fields, and choosing exactly which of its elements receive them:

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

    style {
        .row   { display: flex; gap: .sm; }
        .title { margin: 0; }
        .price { color: .muted; }
    }
}

// In the parent:
CartRow(item: item)
    .container(.highlighted when item.id == selectedId)
    .title(.strikethrough when item.isRemoved)
```

- `@Stylable` fields are optional, so they become chained methods automatically.
- The parent passes classes from **its own** `style` block, still typed and scoped.
- A `@Stylable` field that is never applied to an element is a compile error.
- **There is no `::part()` and no `:deep()`.** A component without `@Stylable` fields can't be styled from
  outside. The parent can still position it by wrapping it, or by placing it in a grid.

## Themes

Global style values live in **one typed `theme` per app**:

```k
// src/theme/AppTheme.k
public theme AppTheme {
    color primary: Color = #3B82F6
    color surface: Color = #FFFFFF
    color surfaceHover: Color = #F3F4F6
    color muted: Color = #6B7280

    space sm: Spacing = 8px
    space md: Spacing = 16px

    radius md: Radius = 8px
    radius lg: Radius = 16px

    font body: Font = Font(family: "Inter", size: 16px)

    variant dark {
        surface = #111827
        surfaceHover = #1F2937
    }
}
```

- **Each token becomes a static member of its type.** `color surfaceHover` adds `Color.surfaceHover`, so
  styles write `backgroundColor: .surfaceHover;`.
- **Token categories are distinct types**, all subtypes of `Length` where they are lengths: `Spacing`,
  `Radius`, `FontSize` and so on. `padding` accepts a `Spacing` or a raw `Length`; `borderRadius` accepts
  a `Radius`. Using a `Color` token for `padding` is a compile error, and `space.md` and `radius.md`
  don't clash.
- **Tokens compile to CSS custom properties** on `:root`.
- **Tokens work in code too:** `canvas.fill(.primary)`.
- **There is one theme per app, with standard-library defaults.** Libraries declare the tokens they need,
  and a token that nothing provides is a compile error. An app overrides a library's token by declaring it
  in its own theme with the same category and type.
- `font` tokens produce font preload tags automatically.

### Variants

A `variant` overrides existing tokens, with the same types. It can't add new ones:

```k
variant dark {
    surface = #111827
    surfaceHover = #1F2937
}
```

**Switching variants is a single class swap on the document root**, with no re-render:

```k
import k.ui.Theme

Theme.use(.dark)
Theme.use(.base)
```

> **Proposal.** `Theme.use`, `Theme.current` and the generated `ThemeVariant` enum follow
> `docs/design/proposals/theme-variants.md`.

The compiler generates a `ThemeVariant` enum with `.base` for the tokens as declared and one case per
variant. `Theme.current` is reactive. A good pattern is to keep the user's choice in a service and apply
it with an effect in a layout that is always mounted:

```k
@Effect
func applyColorScheme() {
    Theme.use(if appearance.scheme == .dark { .dark } else { .base })
}
```

## Colors

`Color` values have methods, computed at compile time where possible and turned into `color-mix()` or
custom properties where not:

```k
.primary.opacity(0.8)
.primary.mix(.white, 20%)
Color.hex("#3B82F6")
Color.rgb(59, 130, 246)
```

## Units in styles

Lengths, percentages, durations and angles in styles are the same [unit literals](../language/units.md)
as in code. `16px * 2` is folded to `32px`; `100px + 2rem` becomes `calc(100px + 2rem)`.
