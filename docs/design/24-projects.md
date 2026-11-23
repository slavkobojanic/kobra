# Projects, `k.toml` and `k new`

## Decision

Every Kobra project, app or package, has a `k.toml` at its root and the same fixed layout. `k new` creates both. `k.toml` is the only configuration file, and it holds only what can't be expressed in Kobra code.

```toml
[package]
name = "weather"                 # the app's identity; a package's module path
title = "Weather"                # display name (optional)
description = "A weather dashboard backed by the Open-Meteo APIs."
k = "0.1"                        # toolchain requirement: any 0.1.x (48)
version = "0.1.0"                # apps only, optional

[dependencies]
"github.com/acme/charts" = "v1.2.0"     # module path = git tag (written by `k add`)

[aliases]
charts = "github.com/acme/charts"       # a short prefix: `import charts.LineChart`
```

## `k.toml`

| Section | Contents |
|---|---|
| `[package]` | `name` and `k` are required (a workspace member may leave `k` to the root's `[workspace] k`). `k` is a toolchain requirement with Cargo's caret meaning: `"0.1"` is any 0.1.x, `"=0.1.2"` exactly that version ([48-releases-and-versioning.md](48-releases-and-versioning.md#the-requirement-and-the-lock)); it replaced an earlier "minimum toolchain version" reading. `title` and `description` are optional. Apps may set `version`, which `dist/app` reports in its logs and metrics; packages can't, because their git tag is their version ([18-packages.md](18-packages.md)). |
| `[dependencies]` | Module path → git tag. `k add` and `k remove` edit it; `k.lock` pins commits and hashes. |
| `[aliases]` | Short name → a git package's module path prefix, which is a URL ([18-packages.md](18-packages.md)). Workspace libraries need none: their modules keep their own paths. |
| `[dev]` | Development settings, read only by `k dev`, `k run` and `k migrate`: values for config variables, generated development secrets and a seed script ([below](#development-settings)). Apps only; a workspace root has none (each member has its own). |

### `name` is the project's identity

- **For an app,** `name` is lower case ASCII letters, digits and hyphens, starting with a letter (`shop`, `my-shop`). **For a package,** it is the module path (`github.com/acme/charts`).
- **Things outside the codebase are derived from it:** the Protobuf package prefix of every protocol ([28-rpc-wire-format.md](28-rpc-wire-format.md#names)) and the namespace of the app's browser storage ([36-local-storage.md](36-local-storage.md#keys)). The storage namespace is recorded in `storage.lock` when first derived and read from the lock afterwards, so renaming the project doesn't change it. Wire names are always derived and recorded in `rpc.lock`, so renaming a project that serves protocols fails `k check` (`K8001`) unless its protocols are marked `@Breaking` ([13-tooling.md](13-tooling.md#lockfiles)).
- **Mapping to Protobuf:** lower case; `-`, `/` and `.` become `_`; a package's host is dropped. `my-shop` → `my_shop`, `github.com/acme/charts` → `acme_charts`.
- `title` is what people see (the default `manifest` name, for example). Changing it changes nothing else.

### What `k.toml` doesn't hold

- **No build scripts, no production configuration, no secrets.** Per-environment choices are `use … in .production` rules in `App.k` ([23-app-structure.md](23-app-structure.md)); configuration comes from the environment ([25-config-and-secrets.md](25-config-and-secrets.md)). The only values in `k.toml` are the development ones of `[dev]`, which no build reads.
- **No tool settings.** The formatter, compiler and test runner have no options to configure.

## Development settings

**Every example and every new app starts with one command, `k dev`, and no setup.** What the process environment and `.env` leave unset, a development run takes from `[dev]` and from the toolchain's own development defaults:

```toml
[dev]
seed = "scripts/demo-catalog.sql"           # run once, when `k dev` creates the database

[dev.env]                                    # development values, committed
SHOP_ADMIN_EMAIL = "admin@shop.example"
PAYBOX_CALLBACK_URL = "http://127.0.0.1:{port}"

[dev.secrets]                                # generated on first use, never committed
SHOP_ADMIN_PASSWORD_HASH = "password-hash"
K_SERVICE_TOKEN = "token"
```

- **`[dev.env]`** gives config variables development values. `{port}` and `{origin}` in a value stand for the app's development port and origin (`http://localhost:<port>`), so an app can name its own address (a callback URL) whatever `--port` says. **A `[dev.env]` value for a `Secret` field is refused at startup** (exit 78): secrets are never committed.
- **`[dev.secrets]`** names secret variables that `k dev` generates on first use and keeps in the project's git-ignored `.k/secrets/<NAME>`, so they survive restarts. `"token"` is 32 random bytes (base64url). `"password-hash"` is the Argon2id PHC string of a random password, which is kept next to it (`<NAME>.password`) and printed at every start, so a developer can sign in as the configured administrator.
- **`seed`** is a SQL file, relative to the project, that `k dev` and `k run` run in one transaction right after they create the development database (when every migration was applied by that start). It is for demo data, which doesn't belong in migrations ([19-persistence.md](19-persistence.md#data-migrations)).
- **Precedence**, highest first: the process environment; a workspace's wiring (`k dev` at a root, [40-workspaces-and-services.md](40-workspaces-and-services.md#tooling)); `.env`; `[dev.env]`; `[dev.secrets]`; the development defaults ([25-config-and-secrets.md](25-config-and-secrets.md#per-environment)); the field's default.
- **`dist/app` and `k test` never read `[dev]`.** A production build still needs every required variable set explicitly, and tests bind configs per test group.
- `k check` validates the section: unknown keys, a non-string value, and a secret kind other than `"token"` or `"password-hash"` are `K8101`.

## Fixed project layout

```
k.toml           manifest
App.k            the app declaration (apps only)
src/             source; each directory is a module (src/weather/Place.k → weather.Place)
migrations/      database migrations, NNNN_description.k; not a module (19-persistence.md)
fonts/           by convention: .woff2 files the theme ships (07, "Shipped fonts"); any directory works
routes.lock      generated and committed
schema.lock      generated and committed
storage.lock     generated and committed
rpc.lock         written by `k build`, checked by `k check`, committed; apps that serve protocols (40)
k.lock           the toolchain that last built the app (written by `k build`, committed); later also the package lock
.env             optional, git-ignored; loaded by `k dev`, `k run` and `k migrate` only
.env.example     committed; documents what .env may contain
.k/              the toolchain's local state (the development database, generated development
                 secrets, caught mail, caches); git-ignored
dist/            build output; git-ignored
```

- **The layout is fixed**, like the formatter: there is no `source = "src"` key. Every Kobra repository looks the same.
- **A project with an `App.k` is an app; one without is a library package.**
- **A package's `.woff2` and `.woff` files are its assets,** wherever they are: a theme's `FontFace("fonts/Inter.woff2")` names one relative to the package root, and every app that uses the theme serves it ([07](07-themes-and-units.md#shipped-fonts)).

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

- **The `app` is named after the project in upper camel case** (`my-shop` → `MyShop`), and `title` is the same words with spaces (`My Shop`). A name whose app would clash with a declaration of the skeleton gets an `App` suffix (`k new home` declares `HomeApp`, beside the page `Home`).
- **A name is checked before anything is written:** lower case ASCII letters, digits and hyphens, starting with a letter, with no empty part between hyphens and no hyphen at the end (`my-shop`, `shop-2-go`; not `my--shop` or `shop-`). The argument is a name, not a path: `k new` creates the directory in the current one.
- **The skeleton is embedded in the binary** (`crates/k_cli/templates/app/`, filled in by `crates/k_cli/src/new.rs`), and `crates/k_cli/tests/new.rs` checks that a new app passes `k check` (its test included), `k test` and `k build`. `k new` prints the two commands to run next (`cd <name>`, `k dev`).
- **`k new --library` is built later** (Planned `k-new-library`); the app skeleton came first because the quick start needs it ([49-documentation.md](49-documentation.md#the-quick-start)).
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
            heading(level: 1) { "Hello from Kobra" }

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
- Keeping the manifest minimal follows "one obvious way": anything that *can* be Kobra code (DI choices, layouts, metadata) *is* Kobra code, where the compiler checks it.
- Recording names derived from `name` in lockfiles follows "lockfiles for anything outside systems depend on": rpc URLs and users' stored data must not move because a project was renamed.
- A fixed skeleton is the project-level counterpart of the configuration-free formatter.

## Open questions

- Whether `k new` should accept `.` to initialize the current, empty directory.
- Whether `[dev.secrets]` needs more kinds (an RSA key, a key pair) once an example needs one.
