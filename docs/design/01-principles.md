# Design Principles

These principles decide individual syntax questions. When a new decision comes up, it should follow from these rather than from whatever another language happens to do.

## Explicit over brief

Agents write most code now, and humans review it. Code is read far more than it is written, so K favors explicit wording over terseness.

## Keywords over symbols

- Use English keywords where other languages use symbols: `bind quantity` instead of `$quantity`, `.expanded when isExpanded` instead of a ternary.
- **There are no ternaries.** Use `if` expressions: `let label = if done { "Done" } else { "Pending" }`.
- Symbols are kept only where they are universal or carry K's core mechanisms:
  - `@` for annotations
  - `?` for optional types, `?.` for optional chaining
  - `=>` for lambdas
  - `.name` for implicit member expressions (see [08-types.md](08-types.md))
  - `{ }` for "a K expression goes here", in strings and in styles
- CSS syntax is kept inside `style` blocks, apart from property names (see [06-styles.md](06-styles.md)).

## One obvious way

Each thing should have one way to do it. Agents produce more consistent code when the language offers no equivalent alternatives, and codebases look alike.

## Keywords declare kinds; annotations modify behavior

- A **keyword** declares a kind of thing with its own compiler semantics: `component`, `page`, `layout`, `service`, `protocol`, `entity`, `repository`, `struct`, `enum`, `class`, `interface`, `theme`.
- An **annotation** modifies behavior within a kind: `@State`, `@Derived`, `@Effect`, `@Prop`, `@Binding`, `@Slot`, `@Stylable`, `@Inject`, `@Scope`, `@Param`.
- **Declaration clauses** read as prose: `page Admin requires Authenticated at "/admin"`, `service CartApi implements CartProtocol`.

## No abbreviations, unless the abbreviation is the established term

- Spelled out: `paragraph` (not `p`), `description` (not `alt`), `numberedList` (not `ol`).
- Kept as they are, because programmers already know them: `enum`, `struct`, `rpc`, `async`, and Swift's `func`, `let` and `var`.

## Mutability is always written out

Declarations follow Swift: `let` for constants, `var` for variables. Every declaration states its mutability, including annotated fields. The annotation determines which keyword is allowed, and a mismatch is a compile error with a quick fix.

```k
@State var isOpen: Bool = false      // @State, @Binding, @Form → var
@Prop let label: String              // @Prop, @Derived, @Load, @Inject, @Param, @Slot, @Stylable, @Emits → let
@Prop var label: String              // compile error: @Prop is read-only; use let
```

**`let` also binds names out of optionals and patterns,** as in Swift. Pattern bindings are always immutable, so there is no `if var`.

```k
if let footer { footer }
guard let user = session.user else { throw AuthError.notLoggedIn }
case .loaded(let product) { ProductCard(product: product) }
catch .outOfStock(let available) { ... }
```

Signatures: `->` for return types; effects in a fixed order, `async throws E -> T`.

## Required values are named arguments; optional values are chained

The Java builder pattern, built into the language:

```k
image(source: user.avatarUrl, description: "Profile photo of {user.name}")
    .class(.avatar)
    .loading(.lazy)
```

This rule covers HTML elements, components, pages and types annotated `@Builder`. Plain functions use ordinary default parameters. See [04-components.md](04-components.md).

## Compile time over runtime

Whatever can be resolved statically is: dependency injection, URLs, class references, Protobuf field numbers, reactive dependency hints. A mistake that would be a runtime bug elsewhere is a compile error in K.

## Lockfiles for anything outside systems depend on

When something outside the codebase depends on a generated value, the compiler records it in a committed lockfile and detects breaking changes:

- `protocol.lock` records Protobuf field numbers ([11-protocols-and-rpc.md](11-protocols-and-rpc.md))
- `routes.lock` records URLs ([12-routing.md](12-routing.md))
- `schema.lock` records the database schema ([19-persistence.md](19-persistence.md))

## Errors for machines and humans

Compiler errors are structured and machine-readable, with quick fixes an agent can apply directly. For example:

> `value` is a @Prop. Change it to @Binding to allow writes.
