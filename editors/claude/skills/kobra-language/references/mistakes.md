# Habits that fail in Kobra

Every row was checked against the compiler. The code is the first diagnostic `k check` reports.
`explain_diagnostic` (MCP) or `search_docs` with the code gives the full explanation.

| You might write (TS/Swift/React) | Kobra | Diagnostic |
|---|---|---|
| `a && b`, `a \|\| b` | `a and b`, `a or b` | K0104, K0105 |
| `!a` | `not a` | K0106 |
| `a ? "yes" : "no"` | `if a { "yes" } else { "no" }` | K0107 |
| `x == true`, `x == false` | `x`, `not x` | K2012 (warning; `k check` still fails) |
| `let a = 1; let b = 2` | one statement per line | K0103 |
| `names.map { $0.count }` | `names.map { name => name.count }` | K0001 (`$`) |
| `{ name => return name.count }` | `{ name => name.count }` | K2308 |
| `a!` (force unwrap) | `if let`, `guard let`, `??`, `?.` | K0102 and K2001 |
| `try? g()` | `try g() else { … }` or `Result.of { try g() }` | K0107 (read as a ternary) |
| `if var b = a` | `if let b = a`, then `var copy = b` | K0122 |
| `case _ { … }` in `match` | `default { … }` | K2303 |
| non-exhaustive `match` | handle every case, or add `default` | K2301 |
| `func f() throws` | `func f() throws SomeError` | K0129 |
| calling a throwing function bare | `try g()` | K2201 |
| `struct P: Codable` | `struct P implements Codable` | K0100 |
| top-level `var counter = 0` | state in a `service` | K0131 |
| `"\{"` | `"{{"` | K0007 |
| `import k.ui.*` | one import per declaration | K0100 |
| `public let a` inside a public type | `let a` (members follow the type) | K1302 (warning) |
| a second `public` declaration, or a file named differently | one public declaration per file, named after it | K1002 |
| `@Prop var label` | `@Prop let label` | K3001 |
| `Child(value: count)` for a `@Binding` | `Child(value: bind count)` | K3016 |
| `bind` on a `@Derived` value or its loop variable | bind `@State`, `@Binding` or `@Form` paths | K3015 |
| `div`, `span`, `p`, `img`, `a` | `box`, `text`, `paragraph`, `image`, `link` | K3101 |
| `for name in names { … }` in a view | `for name in names key name { … }` | K3106 |
| `.on(click: save)` where `save` throws | handle errors inside, and store them in `@State` | K3017 |
| `@Effect func load() throws E` | `@Effect func load() async { do { … } catch … }` | K3012 |
| `@Inject let api: SomeProtocol` in a component | inject a service that wraps the protocol | K4005 |
| `justify-content: space-between` | `justifyContent: spaceBetween` | K3201 |
| `.class(.missing)` | define `.missing` in this declaration's `style` | K3203 |
| `.class(.a).class(.b)` | `.class([.a, .b])` | K3104 |
| `100px > 2rem` | lengths can't be ordered; `==` works | K2013 |
| `rpc ping() -> Int` | `rpc ping() throws RpcOnlyError -> Int` | K5501 |
| `requires RateLimit(…)` without an import | `import k.auth.RateLimit` (and `Authenticated`, `Role`) | K1201 |
| a page without a `title` anywhere | `metadata { title("…") }` on the page, its layout or the app | K3027 (warning) |
| `Date.now`, `setTimeout` | the injected `Clock`: `clock.now`, `clock.after(…)` | K1203 (`Date` has no member `now`) |
| `fetch(url)` | the injected `HttpClient`, wrapped in a service | K1201 |
| `null` | `nil` | K1201 |

## Also not Kobra

- There's no `undefined`: a name can't be used before it has a value.
- There's no inheritance (`extends`, `super`). Use interfaces with default implementations, and
  composition.
- There's no ad-hoc structural typing. A type conforms only by writing `implements`.
- There are no JSX, no hooks (`useState`, `useEffect`), no dependency arrays and no virtual DOM. Use
  `@State`, `@Derived` and `@Effect`.
- There's no `export`. A declaration is `public` or not.
- There's no `async function` or `.then()`. Write `func f() async -> T` and `await f()`.
- There's no `try … catch (e)` with untyped `e`. `catch` clauses match typed cases.
- There are no string URLs for pages. Pages are `Page` enum cases (`link(to: .orderDetail(id: id))`).
- There's no `fetch` to your own server and no REST. The browser calls the server through a
  `protocol` (RPC).
- There are no environment checks in code (`if production`). Use `use … for … in .production` rules
  in `App.k`, and `config` declarations.
