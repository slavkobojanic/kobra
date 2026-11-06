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
- **Tokens compile to CSS custom properties on `:root`.** Switching variants (`Theme.use(.dark)`) is a single class swap with no re-render.
- **Tokens work in code too:** `canvas.fill(.primary)`.
- **A `variant` may only override existing tokens, with the same type.**
- **There is one `theme` per app, with stdlib defaults.** Libraries declare the tokens they need. A missing token is a compile error.

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
