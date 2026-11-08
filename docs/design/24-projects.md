# Projects, `k.toml` and `k new`

## Decision

Every K project, app or package, has a `k.toml` at its root and the same fixed layout. `k new` creates both. `k.toml` is the only configuration file, and it holds only what can't be expressed in K code.

```toml
[package]
name = "weather"                 # the app's identity; a package's module path
title = "Weather"                # display name (optional)
description = "A weather dashboard backed by the Open-Meteo APIs."
k = "0.1"                        # minimum K toolchain version
version = "0.1.0"                # apps only, optional

[dependencies]
"github.com/acme/charts" = "v1.2.0"     # module path = git tag (written by `k add`)

[aliases]
charts = "github.com/acme/charts"       # enables `import charts.LineChart`
```

## `k.toml`

| Section | Contents |
|---|---|
| `[package]` | `name` and `k` are required. `title` and `description` are optional. Apps may set `version`, which `dist/app` reports in its logs and metrics; packages can't, because their git tag is their version ([18-packages.md](18-packages.md)). |
| `[dependencies]` | Module path → git tag. `k add` and `k remove` edit it; `k.lock` pins commits and hashes. |
| `[aliases]` | Short name → module path ([18-packages.md](18-packages.md)). |

### `name` is the project's identity

- **For an app,** `name` is lower case ASCII letters, digits and hyphens, starting with a letter (`shop`, `my-shop`). **For a package,** it is the module path (`github.com/acme/charts`).
- **Things outside the codebase are derived from it:** the Protobuf package prefix of every protocol ([28-rpc-wire-format.md](28-rpc-wire-format.md#names)) and the namespace of the app's browser storage ([36-local-storage.md](36-local-storage.md#keys)). The storage namespace is recorded in `storage.lock` when first derived and read from the lock afterwards, so renaming the project doesn't change it. Wire names are always derived and recorded in `rpc.lock`, so renaming a project that serves protocols fails `k check` (`K8001`) unless its protocols are marked `@Breaking` ([13-tooling.md](13-tooling.md#lockfiles)).
- **Mapping to Protobuf:** lower case; `-`, `/` and `.` become `_`; a package's host is dropped. `my-shop` → `my_shop`, `github.com/acme/charts` → `acme_charts`.
- `title` is what people see (the default `manifest` name, for example). Changing it changes nothing else.

### What `k.toml` doesn't hold

- **No build scripts, no environment variables, no secrets.** Per-environment choices are `use … in .production` rules in `App.k` ([23-app-structure.md](23-app-structure.md)); configuration comes from the environment ([25-config-and-secrets.md](25-config-and-secrets.md)).
- **No tool settings.** The formatter, compiler and test runner have no options to configure.

## Fixed project layout

```
k.toml           manifest
App.k            the app declaration (apps only)
src/             source; each directory is a module (src/weather/Place.k → weather.Place)
migrations/      database migrations, NNNN_description.k; not a module (19-persistence.md)
routes.lock      generated and committed
schema.lock      generated and committed
storage.lock     generated and committed
rpc.lock         generated and committed by apps that serve protocols (40-workspaces-and-services.md)
k.lock           package lock, generated and committed
.env             optional, git-ignored; loaded by `k dev` only
.env.example     committed; documents what .env may contain
.k/              the toolchain's local state (the development database, caches); git-ignored
dist/            build output; git-ignored
```

- **The layout is fixed**, like the formatter: there is no `source = "src"` key. Every K repository looks the same.
- **A project with an `App.k` is an app; one without is a library package.**

## Creating a project: `k new`

```bash
k new hello                                  # an app in ./hello
k new --library github.com/acme/charts       # a library package in ./charts
```

- **`k new <name>` creates an app** in a new directory `<name>/`, with `name` set to `<name>`.
- **`k new --library <module path>` creates a library package.** The directory is named after the last path segment (`charts/`).
- **There are no templates and no other options.** Every new app starts from the same skeleton, and every new library from the same smaller one. Adding a database, a protocol or a theme variant is ordinary code.
- `k new` refuses to write into an existing directory that isn't empty. It doesn't initialize version control; the generated `.gitignore` is ready for when you do. It writes no lockfiles.
- **Every generated file is already formatted and passes `k check`,** including the test.

### The app skeleton

```
hello/
  k.toml                 [package] name, title, description, k
  App.k                  public app Hello { metadata { … } }
  .gitignore             dist/, .k/ and .env
  src/
    home/
      Home.k             public page Home at "/"
      Home.test.k        one component test for Home
    theme/
      AppTheme.k         public theme AppTheme with a small set of tokens
```

- **The `app` is named after the project in upper camel case** (`my-shop` → `MyShop`).
- `Home` uses a named URL because it is the site root, which a derived URL (`/home`) can't express ([12-routing.md](12-routing.md)).

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

## Why

- TOML is readable, has comments, and is familiar from Cargo, which the toolchain is built with.
- Keeping the manifest minimal follows "one obvious way": anything that *can* be K code (DI choices, layouts, metadata) *is* K code, where the compiler checks it.
- Recording names derived from `name` in lockfiles follows "lockfiles for anything outside systems depend on": rpc URLs and users' stored data must not move because a project was renamed.
- A fixed skeleton is the project-level counterpart of the configuration-free formatter.

## Open questions

- Whether `k new` should accept `.` to initialize the current, empty directory.
