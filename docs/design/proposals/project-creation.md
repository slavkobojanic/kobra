# Proposal: Creating Projects (`k new`) and Test Watch Mode

> **Status: proposal.** Not yet accepted into the design. First used by the language guide in `docs/book` (Getting started).

## Problem

[13-tooling.md](../13-tooling.md) lists the commands for working on a project (`k dev`, `k build`, `k check`, `k test`, …) but none for starting one, and [k-toml.md](k-toml.md) fixes the project layout without saying how a new project gets it. [21-testing.md](../21-testing.md) says `k test` has "a watch mode" without naming it. A getting-started guide needs both, and agents need one obvious way to create a project that already matches the layout, the formatter and the lint rules.

## Proposal

### `k new`

```bash
k new hello                                  # an app in ./hello
k new --library github.com/acme/charts       # a library package in ./charts
```

- **`k new <name>` creates an app** in a new directory `<name>/`. The name must be lower case ASCII letters, digits and hyphens, starting with a letter (`hello`, `my-shop`). It becomes `[package] name` in `k.toml`.
- **`k new --library <module path>` creates a library package.** The module path is where the package will live ([18-packages.md](../18-packages.md)), and it becomes `[package] name`, the module path prefix [k-toml.md](k-toml.md) describes. The directory is named after the last path segment (`charts/`).
- **There are no templates and no other options.** Every new app starts from the same skeleton, and every new library from the same smaller one, so "one obvious way" holds from the first file. Adding a database, a protocol or a theme variant is ordinary code.
- `k new` refuses to write into an existing directory that isn't empty. It doesn't initialize version control; the generated `.gitignore` is ready for when you do.
- `k new` writes no lockfiles. The toolchain creates each one the first time it has something to record.

### The app skeleton

```
hello/
  k.toml                 [package] name, version "0.1.0", description, k (the toolchain's version)
  App.k                  public app Hello { metadata { … } }
  .gitignore             dist/, .k/ and .env
  src/
    home/
      Home.k             public page Home at "/"
      Home.test.k        one component test for Home
    theme/
      AppTheme.k         public theme AppTheme with a small set of tokens
```

- **The `app` is named after the project in upper camel case** (`my-shop` → `MyShop`), following [app-structure.md](app-structure.md).
- `Home` uses a named URL because it is the site root, which a derived URL (`/home`) can't express ([12-routing.md](../12-routing.md)).
- **Every generated file is already formatted and passes `k check`,** including the test. A new project's first `k check` is green.
- `.k/` is the toolchain's local state (the development SQLite database, caches), as in the examples; `.env` is the developer's local configuration ([config-and-secrets.md](config-and-secrets.md)). Both are ignored.

The generated `Home.k`:

```k
/// The app's start page.
public page Home at "/" {
    @State var count: Int = 0

    metadata {
        title("Hello")
    }

    view {
        main {
            heading(level: 1) { "Hello from K" }

            paragraph { "Count: {count}" }

            button { "Add one" }
                .on(click: { count += 1 })
        }
        .class(.page)
    }

    style {
        .page {
            maxWidth: 40rem;
            margin: 0 auto;
            padding: .lg;
            font: .body;
        }
    }
}
```

### The library skeleton

```
charts/
  k.toml
  .gitignore
  src/
    charts/
      Example.k          a documented public function, so `k check` has something to check
      Example.test.k
```

A library has no `App.k`; that is what makes it a library ([k-toml.md](k-toml.md)).

### Test watch mode: `k test --watch`

- **`k test --watch`** runs the tests, then re-runs the affected ones whenever a source file is saved. "Affected" comes from the compiler's incremental queries ([13-tooling.md](../13-tooling.md)): a test re-runs when anything it transitively depends on changed.
- `k check` has no watch mode. During development, `k dev` already rebuilds on save and reports diagnostics; `k check` is the full, one-shot gate for CI.

## Why

- A fixed skeleton is the project-level counterpart of the configuration-free formatter: every K project looks the same from its first commit.
- Generating a passing test and a theme from the start makes the conventions (tests next to sources, one theme per app) the path of least resistance.
- A flag rather than a separate command keeps one command per job.

## Open questions

- When each lockfile is first written and updated (on every `k build`, or only by an explicit command), which the lockfile designs leave open.
- Whether `k new` should accept `.` to initialize the current, empty directory.
