# K Language Design

K is a strongly typed, compiled language for building full-stack web applications. It compiles to JavaScript and is designed as a modern successor to Vue, Svelte, React and Angular.

It takes inspiration from:

- **Vue**: single-declaration components with scoped styles.
- **Swift**: readable declarative views, `@State`/`@Binding`, implicit member expressions, `guard`.
- **Java / Spring**: annotations, builders, services, dependency injection, middleware.
- **Rails**: readable number extensions such as `10.minutes`.
- **Rust**: sum types with exhaustive matching.

K borrows ideas, not syntax. Where another language does something well, K adapts it to its own principles.

> **Status:** design phase. Each file below records decisions made so far, with the reasoning behind them. Syntax marked *provisional* is not final.

## Topics

| # | Topic | File |
|---|---|---|
| 1 | Design principles | [01-principles.md](01-principles.md) |
| 2 | Compilation model | [02-compilation.md](02-compilation.md) |
| 3 | Reactivity | [03-reactivity.md](03-reactivity.md) |
| 4 | Components | [04-components.md](04-components.md) |
| 5 | Views and elements | [05-views.md](05-views.md) |
| 6 | Styles | [06-styles.md](06-styles.md) |
| 7 | Themes and units | [07-themes-and-units.md](07-themes-and-units.md) |
| 8 | Types and control flow | [08-types.md](08-types.md) |
| 9 | Errors | [09-errors.md](09-errors.md) |
| 10 | Services and dependency injection | [10-services.md](10-services.md) |
| 11 | Protocols and RPC | [11-protocols-and-rpc.md](11-protocols-and-rpc.md) |
| 12 | Pages and routing | [12-routing.md](12-routing.md) |
| 13 | Tooling | [13-tooling.md](13-tooling.md) |
| 14 | Async data | [14-async-data.md](14-async-data.md) |
| 15 | Forms and validation | [15-forms-and-validation.md](15-forms-and-validation.md) |
| 16 | Modules and visibility | [16-modules.md](16-modules.md) |
| 17 | JavaScript interop | [17-javascript-interop.md](17-javascript-interop.md) |
| 18 | Packages | [18-packages.md](18-packages.md) |
| 19 | Persistence | [19-persistence.md](19-persistence.md) |
| 20 | Standard library | [20-standard-library.md](20-standard-library.md) |
| 21 | Testing | [21-testing.md](21-testing.md) |
| 22 | Document metadata | [22-metadata.md](22-metadata.md) |

## Deferred topics

The first design pass is complete. These were explicitly deferred and may be revisited:

- Typed npm interop ([17-javascript-interop.md](17-javascript-interop.md))
- A hosted package registry ([18-packages.md](18-packages.md))
- An edge deployment target ([13-tooling.md](13-tooling.md))
- A design-system component library ([20-standard-library.md](20-standard-library.md))

Not yet covered in depth: the structure of `App.k`, the auth and session model, animation API details, internationalization workflow, security defaults (CSRF, CORS, content security policy), and concurrency (workers).
