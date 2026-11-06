# Constants and variables

## `let` and `var`

A constant is declared with `let`, a variable with `var`:

```k
let maximumAttempts = 3
var attempts = 0

attempts += 1          // fine
maximumAttempts = 4    // compile error: maximumAttempts is a let
```

**Prefer `let`.** Most values never change after they're created, and a `let` tells the reader so.

## Type annotations

K infers types from initial values. Write the type when there is no initial value, when the inferred
type isn't the one you want, or when the declaration is part of an API:

```k
let name = "Ana"                         // String
let ratio = 0.75                         // Double
let names: List<String> = []             // an empty list needs its element type
var selected: Product? = nil             // an optional, initially empty
let delay: Duration = 200ms
```

A name can't be used before it has a value. The compiler checks this, so there is no "undefined".

## Annotated fields

Inside a component, page, service or other declaration, fields can carry annotations that give them
special behavior, such as reactivity or injection. **The annotation decides whether the field is `let`
or `var`**, and you still write the keyword:

```k
component Stepper {
    @Binding var value: Int          // @Binding, @State, @Form and @Version are always var
    @Prop let step: Int = 1          // @Prop, @Derived, @Load, @Inject, @Param, @Slot, @Stylable, @Emits and @Env are always let
    @State var isFocused: Bool = false

    private var clicks = 0           // a plain var: ordinary, non-reactive data
}
```

Writing the wrong keyword is a compile error with a quick fix:

> `@Prop` is read-only; use `let`.

This rule means you can always tell, from the declaration alone, whether a field can change.

## Binding names from optionals and patterns

`let` also introduces names when you unwrap an optional or match a pattern. These bindings are always
constants, so there is no `if var`:

```k
if let user = session.user {
    Log.info("signed in as {user.id}")
}

guard let product = await repository.product(id) else {
    throw CartError.productNotFound
}

match state {
    case .loaded(let products) { … }
    default { … }
}
```

See [Control flow](control-flow.md) for `if let`, `guard` and `match`.

## Top-level declarations

Constants and functions may be declared at the top level of a file, outside any type:

```k
let defaultPageSize = 50

func clamp(value: Int, low: Int, high: Int) -> Int { … }
```

**A top-level `var` is a compile error.** Shared mutable state belongs in a
[service](../apps/services.md), where the compiler can see who changes it.

## Static members

Types can have static constants and functions, declared with `static let` and `static func`. They're
often used with [implicit member expressions](types.md#implicit-member-expressions):

```k
struct Padding {
    let size: Length

    static let compact = Padding(size: 4px)
    static let comfortable = Padding(size: 12px)
}

let padding: Padding = .compact
```

Like every member, a static member is visible wherever its type is, unless it is marked `private`.
