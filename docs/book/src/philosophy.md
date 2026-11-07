# Philosophy

Every language makes thousands of small decisions. K makes them by following a short list of
principles, so that new decisions stay consistent with old ones. Knowing these principles makes the
rest of the language predictable: when you wonder how something is written in K, the answer usually
follows from one of them.

## Explicit over brief

Code is read far more often than it is written, and today much of it is written by agents and read by
humans. K therefore prefers explicit wording to terseness. A few extra characters are cheap; a
misunderstanding during review is not.

## Keywords over symbols

K uses English keywords where other languages use symbols:

| Instead of | K writes |
|---|---|
| `$quantity` (two-way binding) | `bind quantity` |
| `isOpen ? "Close" : "Open"` | `if isOpen { "Close" } else { "Open" }` |
| `&&`, `\|\|`, `!` | `and`, `or`, `not` |
| a conditional class expression | `.expanded when isExpanded` |

**There are no ternaries.** `if` is an expression, so it does the same job and reads as prose:

```k
~func example(done: Bool) {
let label = if done { "Done" } else { "Pending" }
~}
```

Symbols survive only where they are universal (`+`, `==`, `<`) or carry one of K's core mechanisms:

- `@` for annotations
- `?` for optional types and `?.` for optional chaining
- `=>` for lambdas
- `.name` for implicit member expressions
- `{ }` for "a K expression goes here", in strings and in styles

Inside `style` blocks, CSS syntax is kept as it is, apart from camelCase property names.

## One obvious way

Each thing has one way to do it. When a language offers several equivalent spellings, every codebase
picks a different one, and agents produce inconsistent code. K's formatter has no options, imports
have no wildcards, queries have one form, and the project layout is fixed. Two K codebases look alike.

When a redundant second spelling still parses, the compiler reports it with a machine-applicable fix:

- `public` on a member, which is already public when its type is ([Modules and visibility](language/modules.md));
- comparing a `Bool` with a literal (`x == false`) instead of writing `not x`;
- a top-level `case _` arm in `match` instead of `default`;
- a `use` rule in a test that restates the default test double ([Testing](apps/testing.md)).

## Keywords declare kinds; annotations modify behavior

- A **keyword** declares a kind of thing with its own compiler semantics: `component`, `page`,
  `layout`, `service`, `protocol`, `entity`, `repository`, `struct`, `enum`, `class`, `interface`,
  `theme`, `app`, `config`, `migration`, `test`.
- An **annotation** modifies behavior within a kind: `@State`, `@Derived`, `@Effect`, `@Prop`,
  `@Binding`, `@Slot`, `@Stylable`, `@Inject`, `@Scope`, `@Param`.
- **Declaration clauses read as prose:** `page Admin requires Authenticated at "/admin"`,
  `service CartApi implements CartProtocol`.

## No abbreviations, unless the abbreviation is the established term

Built-in elements are spelled out: `paragraph`, not `p`; `numberedList`, not `ol`; an image has a
`description`, not an `alt`. Terms that programmers already know keep their short form: `enum`,
`struct`, `rpc`, `async`, and Swift's `func`, `let` and `var`.

## Mutability is always written out

Every declaration says whether it can change: `let` for constants, `var` for variables. This includes
annotated fields, and the annotation decides which keyword is allowed:

```k,ignore
@State var isOpen: Bool = false      // @State, @Binding, @Form, @Version → var
@Prop let label: String              // @Prop, @Derived, @Load, @Inject, @Param, @Slot, @Stylable, @Emits, @Env → let
@Prop var label: String              // compile error: @Prop is read-only; use let
```

`let` also binds names out of optionals and patterns. Pattern bindings are always immutable, so there
is no `if var`:

```k,ignore
if let footer { footer }
guard let user = session.user else { throw AuthError.notLoggedIn }
case .loaded(let product) { ProductCard(product: product) }
catch .outOfStock(let available) { … }
```

## Required values are named; optional values are chained

K builds the Java builder pattern into the language. Required values are named arguments. Optional
values are set with chained calls:

```k
~struct User {
~    let name: String
~    let avatarUrl: Url
~}
~
~component Avatar {
~    @Prop let user: User
~
~    view {
image(source: user.avatarUrl, description: "Profile photo of {user.name}")
    .class(.avatar)
    .loading(.lazy)
~    }
~
~    style {
~        .avatar { borderRadius: .lg; }
~    }
~}
```

The rule covers built-in elements, components, pages and types annotated `@Builder`. The compiler
generates the chained methods, so there are no hand-written builders. Plain functions use ordinary
default parameters instead.

**A boolean option is chained with its value:** `.disabled(isSubmitting)`, `.dashed(true)`,
`.primaryKey(true)`. There are no zero-argument flag methods, so a flag set from a variable and a flag
set to a constant read the same way.

## Compile time over runtime

Whatever can be resolved statically is resolved statically: dependency injection, URLs, class
references, Protobuf field numbers, reactive dependency hints. A mistake that would be a runtime bug in
another stack is a compile error in K. Linking to a page that doesn't exist, styling with a class that
isn't defined, or injecting a database into browser code all fail the build.

- **Validated literals are checked at compile time.** `Url("https://…")`, `Uuid("…")` and
  `EmailAddress("…")` take only a string literal and are never optional. Text known only at runtime goes
  through `Type.parse(…)`, which returns an optional.
- **A distinction the type system can make, it makes,** rather than a runtime check: a `Mac` can't be
  compared with `==`, and an email address can't be invalid once it is an `EmailAddress`.

## Lockfiles for anything outside systems depend on

Some generated values are depended on by things outside your codebase: URLs are bookmarked, Protobuf
field numbers are baked into deployed clients, and database columns hold production data. The compiler
records each of them in a committed lockfile and detects breaking changes:

- `protocol.lock` records Protobuf names and field numbers.
- `routes.lock` records page URLs.
- `schema.lock` records the database schema, including every table and column name.
- `storage.lock` records formats stored outside a request: browser storage keys and background job
  payloads.

A name that something outside depends on is recorded once and then kept: renaming the declaration it
came from never silently changes it.

See [Project files and lockfiles](reference/project-files.md).

## Errors for machines and humans

Compiler errors are structured and machine-readable, and most come with a quick fix that an editor or
an agent can apply directly:

> `value` is a @Prop. Change it to @Binding to allow writes.

## Where the ideas come from

| From | K takes |
|---|---|
| Vue | single-declaration components with scoped styles |
| Swift | readable declarative views, `@State` and `@Binding`, implicit member expressions, `guard` |
| Java and Spring | annotations, builders, services, dependency injection, guards and middleware |
| Rails | readable number extensions such as `10.minutes` |
| Rust | sum types with exhaustive matching |
