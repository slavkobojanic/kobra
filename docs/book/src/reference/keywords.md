# Keywords

Kobra has two kinds of keyword.

- **Hard keywords** are reserved everywhere and can never be used as names.
- **Contextual keywords** are keywords only in specific positions, such as the start of a declaration.
  Everywhere else they are ordinary names, so `let page = …`, `Cache.set(key, value)` and
  `Analytics.send(page, …)` are all valid.

## Hard keywords

| Keyword | Meaning |
|---|---|
| `and`, `or`, `not` | logical operators |
| `as`, `is` | reserved for future use |
| `async` | the function can suspend |
| `await` | wait for an async call; `for await` reads a stream |
| `bind` | pass state for two-way binding: `Stepper(value: bind quantity)` |
| `break`, `continue` | leave a loop, or skip to its next iteration |
| `case` | an enum case declaration, or a pattern in `match` |
| `catch` | handle errors after `do`; `catch let error` binds what remains, `catch _` doesn't |
| `class` | declare a class |
| `default` | the non-binding catch-all arm of `match` |
| `do` | a block whose errors are handled by `catch` |
| `else` | the alternative branch of `if`, `guard` or `try` |
| `enum` | declare an enum |
| `expect` | a test assertion; `expect throws` |
| `extend` | add members to an existing type |
| `false`, `true`, `nil` | literals |
| `for` | a loop; also the target in `use X for Y` and `repository R for E` |
| `func` | declare a function |
| `guard` | early exit when a condition fails |
| `if` | a conditional statement or expression |
| `implements` | declare conformance to an interface, protocol or derived behavior |
| `import` | import one declaration |
| `in` | the collection in `for … in`; environments in `use … in` |
| `init` | an initializer |
| `interface` | declare an interface |
| `let` | declare a constant, or bind a name in a pattern |
| `match` | exhaustive branching |
| `private`, `public` | visibility modifiers; members take only `private` |
| `requires` | guards on a page, a protocol or a protocol's rpc (never on a layout, `K5014`, or a handler, `K5507`) |
| `return` | return from a function |
| `rpc` | declare or implement a remote call |
| `self` | the current instance |
| `static` | a member of the type rather than of an instance |
| `struct` | declare a struct |
| `style` | a component's style block |
| `throw`, `throws` | raise an error; declare a function's error type |
| `try` | mark a call that can throw |
| `use` | choose an implementation (`use StripePayments for PaymentProvider`), or bind a config to a value in a test |
| `var` | declare a variable |
| `when` | a conditional class: `.expanded when isExpanded` |
| `where` | a generic constraint, or a guard on a `match` or `catch` pattern |
| `while` | a loop |

## Contextual keywords

| Keyword | Where it is a keyword | Meaning |
|---|---|---|
| `app` | start of a declaration | the application, in `App.k` |
| `at` | in a `page` header | a named URL: `page Pricing at "/pricing"` |
| `component` | start of a declaration | a UI component |
| `config` | start of a declaration | typed configuration read from the environment |
| `entity` | start of a declaration | a database table |
| `failed` | a member followed by `(` or `{` | what to show when loads fail |
| `key` | after the collection in a `for` | the identity of each element in a view loop |
| `layout` | start of a declaration | a layout that wraps pages |
| `loading` | a member followed by `{` | what to show while loads run |
| `metadata` | a member followed by `{` | document `<head>` entries |
| `migration` | start of a declaration, in `migrations/` | a database migration |
| `page` | start of a declaration | a routable page |
| `proto` | after `import` | import Protobuf definitions (**Planned** protobuf) |
| `protocol` | start of a declaration | an RPC contract |
| `repository` | start of a declaration | database queries for an entity |
| `service` | start of a declaration | injectable logic and shared state |
| `test` | start of a declaration | a test group, or a single test |
| `theme` | start of a declaration | the app's design tokens |
| `variant` | a member of a `theme` | a set of token overrides; `variant dark matches .prefersDark` follows the system |
| `view` | a member followed by `{` | a component's UI |
