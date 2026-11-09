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
- **Selectors, pseudo-classes, `@media`, `@container`, `@layer`, `@keyframes` and nesting are kept from CSS.** Only property names and keyword values change.
- **Keyword values are camel case, like property names:** `justifyContent: spaceBetween`, `textDecoration: lineThrough`, `fontFamily: "Inter", sansSerif`, `transition: backgroundColor 200ms easeInOut`, `@media (prefersReducedMotion: noPreference)`. Code generation prints the CSS spelling (`space-between`). See [Keyword values](#keyword-values).
- **There are no element or id selectors** (grammar [RA-8](../compiler/grammar.md#ra-8-no-element-selectors)): `h1 { … }`, `td { … }` and `#main { … }` are compile errors. Elements are styled through classes, which are typed and scoped: `heading(level: 1) { title }.class(.title)` with `.title { … }`. A selector is built from classes, `&`, pseudo-classes and pseudo-elements, attribute selectors and `*`.
- **Theme tokens and static values use implicit member syntax:** `padding: .md` resolves against the type the property expects. See [07-themes-and-units.md](07-themes-and-units.md).
- **`{ }` is used only for reactive values** (props and state). `{accent}` compiles to a CSS custom property, updated through the signal system. No inline `style=` strings are needed.
- **A reactive value is written as a CSS value of its type**, not as interpolation text: an `Angle` gets `deg` (`rotate: {windArrowAngle}` is `298deg`), a `Duration` gets `ms`, and lengths and colors are CSS text already.
- **Position decides meaning:** at the rule level, `.summary` is a class selector; after `property:`, `.md` is a value.

## Keyword values

**Decision: CSS keywords in values are written in camel case, and the kebab-case CSS spelling is a compile error (`K3214`) whose machine-applicable fix rewrites it.**

- **Why:** property names were already camel case (`justifyContent`), so `justifyContent: space-between` mixed two spellings in one declaration. Outside style blocks `space-between` reads as a subtraction, which is how editors without the compiler's style mode (TextMate grammars, diff viewers, Markdown renderers) colour it. One spelling, the K identifier spelling, everywhere.
- **Mapping:** a keyword maps to CSS by inserting `-` before each upper-case letter and lower-casing it (`lineThrough` → `line-through`, `inlineBlock` → `inline-block`, `systemUi` → `system-ui`). Single-word keywords (`solid`, `nowrap`, `hidden`) are unchanged. `currentColor` is the one exception: CSS spells it that way (and matches keywords case-insensitively), so it prints as written.
- **The same rule applies everywhere a keyword appears in a value:** inside functions (`repeat(autoFill, minmax(14rem, 1fr))`), in comma-separated values, in the property name of `transition` (`transition: backgroundColor 200ms`, which is a property name and so camel case anyway) and in media feature values (`prefersColorScheme: dark`, `prefersReducedMotion: noPreference`).
- **Left alone:** numbers and units, colors, theme tokens and other K values (`.md`, `.muted`), strings (`"Inter"`), function names (`minmax`, `repeat`), and selectors. There are no vendor-prefixed keywords or custom properties in style blocks, so nothing else needs an exception.
- **The tables are the source of truth:** `k_sema::stdlib::css` holds each property's grammar with camel-case keywords and exposes them to tooling (`property_names`, `keywords(property)`, `media_feature_keywords(feature)`, `WIDE_KEYWORDS`), so completion offers exactly the spellings the checker accepts.
- **Rejected:** accepting both spellings (two ways to write the same thing, and the kebab form keeps rendering as a subtraction in editors); passing keywords through verbatim and requiring kebab case (inconsistent with property names, which are camel case for the same reason).

## Class references

- `.class(.muted)` refers to a class in this component's `style` block.
- **Referencing a class that doesn't exist is a compile error. A defined class that is never used is a warning.**
- **Conditional classes use `when`:** `.class([.panel, .expanded when isExpanded])`.
- Setting `.class()` twice is a compile error. Use a list.

## Animations: `@keyframes`

A style block may declare keyframes, which are scoped like classes:

```k
style {
    @keyframes fade {
        from { opacity: 0; }
        50% { opacity: 0.5; }
        to { opacity: 1; }
    }

    .toast {
        animationName: .fade;
        animationDuration: 200ms;
    }
}
```

- **Each block's selectors are `from`, `to` or percentages** (`0%` to `100%`, several separated by commas), and the block holds declarations only.
- **Keyframes are referenced from properties as an implicit member,** `animationName: .fade`, like a theme token. Referencing keyframes that don't exist is a compile error; declaring keyframes that nothing references is a warning, as for classes.
- **The name is hashed like a class name,** so `fade` in one component can't collide with `fade` in another, and a parent can't reference a child's keyframes.

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
