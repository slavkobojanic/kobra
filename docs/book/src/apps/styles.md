# Styles and themes

Each component, page and layout can have one `style { }` block. It keeps **CSS selectors and value
syntax**, and changes one thing: **names are camelCase**, property names and keyword values alike,
mapped one-to-one to CSS (`borderLeft` → `border-left`, `spaceBetween` → `space-between`). The compiler type-checks it, scopes it, and connects it to the theme.

```k
component CartSummary {
    @Prop let accent: Color = .primary
    @State var isExpanded: Bool = false

    view {
        section {
            paragraph { "Nothing here yet." }
                .class([.muted, .small])

            box {
                // …
~                text { "Details" }
            }
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
  Keywords are camelCase like properties: `justifyContent: spaceBetween`, `textDecoration: lineThrough`,
  `fontFamily: "Inter", sansSerif`. The CSS gets the usual spelling (`space-between`); writing the
  kebab-case form is an error (K3214) whose quick fix rewrites it.
- Comments use `//` and `/* */`, as in Kobra code.

Declarations end with `;`, as in CSS. This is the one place in Kobra where semicolons appear.

What isn't kept: **element selectors and `#id` selectors.** Styles select the classes you define, never
`box` or `button`, and `!important` is a compile error.

## Values are type-checked

**Property values are checked against each property's CSS grammar.** `borderLeft: 4px blue solid potato`
is a compile error, not a silently ignored declaration.

Two kinds of Kobra value appear in styles:

- **Theme tokens and static values use implicit member syntax:** `padding: .md` resolves against the type
  `padding` expects, so it means `Spacing.md`. They never need braces.
- **Reactive values use `{ }`:** `{accent}` reads a prop or state. It compiles to a CSS custom property
  that is updated through the signal system, so there is no need for inline `style=` strings, and no
  re-render when it changes.

Position decides meaning: at the start of a rule, `.summary` is a class selector; after `property:`,
`.md` is a value.

## Class references

```k
~component Empty {
~    view {
paragraph { "Nothing here yet." }
    .class(.muted)
~    }
~
~    style {
~        .muted { color: .muted; }
~    }
~}
```

- `.class(.muted)` refers to the `.muted` class in this component's own `style` block.
- **Referencing a class that doesn't exist is a compile error. A class that is defined but never used is
  a warning.**
- **Conditional classes use `when`:** `.class([.panel, .expanded when isExpanded])`.
- Several classes go in one list. Calling `.class()` twice on one element is a compile error.

## Scoping

**Styles are scoped by default.** The compiler hashes class names, so `.panel` in one component can't
collide with `.panel` in another, and no style leaks out of its component. `k dev` names it
`panel_k3x9a1`; `k build` gives every scoped class a short name such as `_3`, so don't select elements
by class name in tests or scripts.

Global CSS is an explicit escape hatch:

```k
~component Legacy {
~    view {
box {
    // …
}
    .class(global("btn-primary"))
~    }
~}
```

## Styling child components: `@Stylable`

**A component is closed to outside styling unless it opts in.** It opts in by declaring `@Stylable`
fields, and choosing exactly which of its elements receive them:

```k
~struct CartItem {
~    let id: Int
~    let name: String
~    let price: Money
~    let isRemoved: Bool
~}
~
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
~component Cart {
~    @Prop let item: CartItem
~    @Prop let selectedId: Int
~
~    view {
CartRow(item: item)
    .container(.highlighted when item.id == selectedId)
    .title(.strikethrough when item.isRemoved)
~    }
~
~    style {
~        .highlighted { backgroundColor: .surfaceHover; }
~        .strikethrough { textDecoration: lineThrough; }
~    }
~}
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

    fontSize sm: FontSize = 14px

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
- **There is one theme per app, with standard-library defaults.** A token that nothing declares is a
  compile error.
- A library's tokens (`k.charts`' chart colours, `k.drag`'s drop colours) are on `:root` with their
  defaults as soon as the app imports from the library.

### Fonts

A `font` token is a family and a size. The compiler always follows the family with a generic
fallback stack, so text never falls back to the browser's default serif where the family isn't
installed: `Font(family: "Inter", size: 16px)` becomes `16px "Inter", system-ui, …, sans-serif`. A
family whose name says `Serif` falls back to serif faces, one that says `Mono` or `Code` to
monospaced ones.

Kobra doesn't load fonts from the web. A named family is drawn where the reader has it installed, and the
fallback stack everywhere else.

### Base styles

Every app's stylesheet starts with a few base rules taken from the theme, so the page looks right
before your own styles say anything:

- an element with the `hidden` attribute stays hidden, whatever `display` a style gives it;
- the body has no margin, and uses the `font body` token (the system's sans-serif stack without one);
- the body's text and background colours are the `text` (else `foreground`, `onBackground` or
  `onSurface`) and `background` (else `surface`) color tokens, so they follow the variant;
- buttons, inputs, selects and text areas inherit the page's font;
- in an app with a theme, `color-scheme` is dark in a variant whose background is dark, so native
  inputs, scrollbars and links are dark too;
- a `ReorderableList` has no bullets.

The base rules are element selectors (`:where(…)` for the list), so any class of yours overrides
them; only the `hidden` rule is `!important`.

### Library tokens

A library declares the tokens it needs, with defaults: `k.charts` declares `color chartGrid`, for
example. The app's theme **overrides an inherited token with the assignment form**, the same form
variants use:

```k
public theme WeatherTheme {
    color primary: Color = #3B82F6 // declares a new token
    chartGrid = #E2E8F0            // overrides the token k.charts declares
    chartAxis = #64748B

    variant dark {
        primary = #60A5FA
        chartGrid = #334155
    }
}
```

- **Declaration always creates; assignment always overrides,** at the top level of the theme and in
  variants alike.
- The overriding value's category and type come from the library's declaration. Assigning a token that
  nothing declares is a compile error.
- Declaring a token with the name of one a library already declares, such as
  `color chartGrid: Color = …`, is a compile error (`K3302`) with a quick fix to the assignment form. So an app
  token can't silently collide with a token a library adds later.

### Variants

A `variant` overrides existing tokens, with the same types. It can't add new ones:

```k
~theme VariantTheme {
~    color surface: Color = #FFFFFF
~    color surfaceHover: Color = #F3F4F6
variant dark {
    surface = #111827
    surfaceHover = #1F2937
}
~}
```

### Switching variants at runtime

The compiler generates an enum, **`ThemeVariant`**, from the app's theme: `.base` for the tokens as
declared, and one case per variant (`.dark`, `.highContrast`, …). Like `Page`, it needs no import, and
`base` can't be used as a variant name.

Variants are switched through **`Theme`**, a client-only standard-library service in `k.ui`. Like any
service, it is injected:

```k
import k.ui.Theme

component ThemeToggle {
    @Inject let theme: Theme

    view {
        button { "Dark mode" }
            .pressed(theme.current == .dark)
            .on(click: toggle)
    }

    func toggle() {
        theme.apply(if theme.current == .dark { .base } else { .dark })
    }
}
```

| Member | Meaning |
|---|---|
| `theme.current: ThemeVariant` | the active variant. Reactive, so views and effects that read it update. |
| `theme.apply(_ variant: ThemeVariant)` | makes `variant` active by swapping one class on the document root, and remembers the choice. Nothing re-renders. |
| `theme.followSystem()` | forgets the explicit choice, so variants that match a system preference apply again |

- **Following the operating system:** `variant dark matches .prefersDark { … }` applies the variant
  through a CSS media query until `theme.apply` is called. Because it is CSS, the first paint is already
  right, even when server rendered. `.prefersContrast` works the same way.
- **An explicit choice is remembered in a cookie** (`k_theme`, holding only the variant name; named `k_theme_<port>` on `http://localhost:<port>` and `__Host-k_theme` on `https`, so apps on one host keep their own choice). Server
  rendering reads it, so a server-rendered app shows the chosen scheme from the first paint, with no
  flash. A static build's `index.html` has a tiny inline script that does the same in the browser
  before anything paints, so a reload keeps the scheme there too. It is the same in every app; a
  Content-Security-Policy allows it by its hash, as `examples/kanban/nginx.conf` does.
- During server rendering, `theme.current` is the remembered variant, or `.base`, and `apply` does nothing.
- A choice that lives in another service, for example a settings service that syncs it to the account,
  is applied from an `@Effect` in a page or layout that is always mounted:

  ```k
  ~import k.ui.Theme
  ~
  ~enum ColorScheme {
  ~    case light
  ~    case dark
  ~}
  ~
  ~service AppearanceService {
  ~    @State var scheme: ColorScheme = .light
  ~}
  ~
  ~component Appearance {
  @Inject let theme: Theme
  @Inject let appearance: AppearanceService

  @Effect
  func applyColorScheme() {
      theme.apply(if appearance.scheme == .dark { .dark } else { .base })
  }
  ~    view {
  ~        text { "…" }
  ~    }
  ~}
  ```

- The method is `apply`, not `use`, because `use` is a keyword and Kobra has no member names that are
  keywords.

In tests, every test gets a fresh `Theme` starting at `.base`, with no cookie set. A test reads the active
variant with `@Inject let theme: Theme` and `theme.current`.

## Colors

`Color` values have methods, computed at compile time where possible and turned into `color-mix()` or
custom properties where not:

```k,ignore
.primary.opacity(0.8)
.primary.mix(.white, 20%)
Color.hex("#3B82F6")
Color.rgb(59, 130, 246)
```

## Units in styles

Lengths, percentages, durations and angles in styles are the same [unit literals](../language/units.md)
as in code. `16px * 2` is folded to `32px`; `100px + 2rem` becomes `calc(100px + 2rem)`.
