# Themes and Units

## Themes

Global style values are declared in a typed `theme`:

```k
// Theme.k
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

- **Tokens become static members of their type.** `color surfaceHover` adds `Color.surfaceHover`, so styles write `backgroundColor: .surfaceHover;`.
- **Token categories are distinct nominal types,** all subtypes of `Length`: `Spacing`, `Radius`, `FontSize`, and so on. `padding` accepts `Spacing` or a raw `Length`; `borderRadius` accepts `Radius`.
  - After `padding: .`, autocomplete lists only spacing tokens.
  - `space.md` and `radius.md` don't clash.
  - Using a `Color` token for `padding` is a compile error.
- **Tokens compile to CSS custom properties on `:root`.** Switching variants (`theme.use(.dark)`, see [below](#switching-variants-at-runtime)) is a single class swap with no re-render.
- **Tokens work in code too:** `canvas.fill(.primary)`.
- **A `variant` may only override existing tokens, with the same type.**
- **There is one `theme` per app, with stdlib defaults.** Libraries declare the tokens they need. A missing token is a compile error.

## Library tokens

A library declares the tokens it needs, with defaults (`k.charts` declares `color chartGrid`, [37-charts.md](37-charts.md)). The app's theme **overrides an inherited token with the assignment form**, the same form variants use:

```k
public theme AppTheme {
    color primary: Color = #3B82F6      // declares a new token
    chartGrid = #E2E8F0                 // overrides the token k.charts declares

    variant dark {
        surface = #111827
        chartGrid = #334155
    }
}
```

- **Declaration always creates; assignment always overrides,** at the top level of the theme and in variants alike.
- The overriding value's category and type come from the library's declaration. Assigning a token that nothing declares is a compile error.
- Declaring a token with the name of one a library already declares is a compile error, with a quick fix to the assignment form. So an app token can't silently collide with a token a library adds later.

## Switching variants at runtime

### `ThemeVariant` is generated

- The compiler generates an enum, **`ThemeVariant`**, from the app's `theme`: **`.base`** for the tokens as declared, and one case per `variant` (`.dark`, `.highContrast`, …). Like the generated `Page` enum ([12-routing.md](12-routing.md)), it needs no import, and an unknown variant is a compile error.
- `base` can't be used as a variant name, because it means "no variant".

### The `Theme` service

`Theme` is a stdlib service in `k.ui`. Like any service, it is injected, so its state has one owner and every test gets a fresh one:

```k
component ThemeToggle {
    @Inject let theme: Theme

    view {
        button { "Dark mode" }
            .pressed(theme.current == .dark)
            .on(click: toggle)
    }

    func toggle() {
        theme.use(if theme.current == .dark { .base } else { .dark })
    }
}
```

| Member | Meaning |
|---|---|
| `theme.current: ThemeVariant` | the active variant. Reactive, so views and effects that read it update when it changes. |
| `theme.use(_ variant: ThemeVariant)` | makes `variant` active by swapping one class on the document root, and remembers the choice (below). Nothing re-renders. |
| `theme.followSystem()` | forgets the explicit choice, so variants that `match` a system preference apply again |

- **`Theme` is client-only** ([10-services.md](10-services.md)). Injecting it into server-only code is a compile error.
- **Following the operating system:** `variant dark matches .prefersDark { … }` applies the variant through a CSS media query (`prefers-color-scheme`) until `theme.use` is called. Because it is CSS, the first paint is already right, even when server rendered. `.prefersContrast` works the same way.
- **An explicit choice is remembered in a cookie** (`k_theme`, not `HttpOnly`, holding only the variant name). Server rendering reads it and renders that variant, so a server-rendered app shows the chosen scheme from the first paint with no flash and no `@Rendering(.client)`.
- **During server rendering,** `theme.current` is the remembered variant, or `.base`, and `use` does nothing. Effects don't run on the server anyway ([03-reactivity.md](03-reactivity.md)).
- A choice that lives in a service (for example a settings service that syncs it to the account) is applied from an `@Effect` in the page or layout that is always mounted: `theme.use(settings.variant)`.

### Tests

Each test gets a fresh container, so `theme.current` starts at `.base` and no cookie is set ([21-testing.md](21-testing.md)). A test reads the active variant with `@Inject let theme: Theme`.

## Colors

`Color` has methods: `.primary.opacity(0.8)`, `.primary.mix(.white, 20%)`, `.hex("#3B82F6")`, `.rgb(59, 130, 246)`. These are computed at compile time where possible, and become `color-mix()` or custom properties where not.

## Unit literals

Units are **native literals in the core language**, not only in styles:

```k
let delay: Duration = 200ms
let width = 100px + 2rem          // Length; compiles to calc(100px + 2rem)
let gutter = 16px * 2             // folded to 32px at compile time
let bad = 100px + 200ms           // compile error: Length + Duration
```

| Literal | Type |
|---|---|
| `16px`, `1.5rem`, `2em`, `50vh`, `100vw`, `1fr` | `Length` (or a subtype) |
| `50%` | `Percent` |
| `200ms`, `2s` | `Duration` |
| `45deg` | `Angle` |
| `#3B82F6` | `Color` |

- Lengths with matching units are folded at compile time. Mixed units become `calc()`.
- Comparing mixed units (`100px > 2rem`) is a compile error outside a layout context, because the result depends on the root font size.

## Number extensions

Units without a CSS suffix are **extension properties on numbers**, as in Rails:

```k
let timeout = 10.minutes
let expiresAt = Date.now + 3.days
let lastSeen = 2.hours.ago
let maxUpload = 25.megabytes

Cache.set(key, value, ttl: 1.hour)
Timer.every(30.seconds) { refresh() }
```

- **CSS units are literal suffixes** (`px`, `rem`, `em`, `%`, `vh`, `vw`, `fr`, `deg`, `ms`, `s`). **Everything else is an extension property** (`.minutes`, `.hours`, `.days`, `.weeks`, `.bytes`, `.kilobytes`, `.megabytes`).
- Singular forms (`1.hour`) are aliases.
- Both forms produce the same types: `200ms == 0.2.seconds`.
- Extensions are a general language feature. Libraries and apps can add their own: `extend Int { var retries: RetryPolicy { ... } }`.
- `Date.now`, `.ago` and `Timer` read the clock described in [35-time.md](35-time.md). How code reaches the clock (an ambient context or an injected `Clock`) is an open question recorded there.

## Open questions

- **Scoped variants**, applying a variant to one subtree (for example a dark sidebar in a light app), would need a `.theme(.dark)` modifier.
