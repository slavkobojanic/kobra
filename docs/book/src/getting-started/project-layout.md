# Project layout

Every K project, app or package, has the same layout. There is no setting to move the source directory,
no build script and no tool configuration, so once you know one K project you know where everything is in
all of them.

```text
k.toml           the manifest
App.k            the app declaration (apps only)
src/             source code; every directory is a module
migrations/      database migrations, NNNN_description.k; not a module
routes.lock      page URLs and redirects (generated, committed)
schema.lock      the database schema (generated, committed)
storage.lock     browser storage keys and job payloads (generated, committed)
rpc.lock         wire names, rpcs and field numbers of the protocols an app serves (generated, committed)
k.lock           exact versions of packages (generated, committed)
.env             local development configuration (optional, git-ignored)
.env.example     documents what .env may contain (committed)
.k/              the toolchain's local state: dev database, caches (git-ignored)
dist/            build output (git-ignored)
```

**A project with an `App.k` is an app; one without is a library package.**

## `k.toml`

The manifest holds only what can't be expressed in K code:

```toml
[package]
name = "weather"                           # the app's identity; a git package's module path prefix
title = "Weather"                          # display name (optional)
description = "A weather dashboard backed by the Open-Meteo APIs."
k = "0.1"                                  # minimum toolchain version
version = "0.1.0"                          # apps only, optional

[dependencies]
"github.com/acme/charts" = "v1.2.0"        # module path = git tag

[aliases]
charts = "github.com/acme/charts"          # a short prefix: `import charts.LineChart`
```

- **`name` and `k` are required.** For an app, `name` is lower-case ASCII letters, digits and hyphens,
  starting with a letter (`shop`, `my-shop`). For a git package, it is the URL that prefixes its module
  paths (`github.com/acme/charts`). A library in your own [workspace](../apps/workspaces.md) keeps its
  modules' own paths, so its name only identifies it in `[dependencies]`.
- **`name` is the project's identity.** The namespace of the app's browser storage is derived from it
  once and recorded in `storage.lock`, so renaming the project doesn't orphan your users' stored data. The
  Protobuf package prefix of every protocol is derived from it too, and `rpc.lock` records the result, so
  renaming a project that serves protocols fails `k check` until the change is marked `@Breaking`.
- **`title`** is what people see, for example the default name in the web manifest. Changing it changes
  nothing else.
- **`version`** is for apps only, and `dist/app` reports it in its logs and metrics. A package can't set
  one: its git tag is its version.

What you won't find in `k.toml` is as important as what you will:

- **No environment variables or secrets.** Configuration comes from the environment at runtime, through
  typed `config` declarations (see [Configuration and secrets](../apps/config.md)), so nothing secret is
  ever committed.
- **No per-environment settings.** Choosing a different implementation in production is a `use` rule in
  `App.k`, where the compiler checks it.
- **No build scripts and no tool settings.** The compiler, formatter and test runner have no options.

## `App.k`

`App.k` holds the one `app` declaration. It only contains choices the compiler can't infer: the default
layout, the sign-in page, app-wide metadata and which implementation of an interface to use in which
environment. Pages, services, configs, jobs and the theme are found from their declarations; nothing is
registered by hand. See [The app declaration](../apps/app.md).

## `src/` and modules

**Every directory is a module, and the module name comes from the path:**

```text
src/
  shop/
    cart/
      CartService.k          module shop.cart
      CartService.test.k     its tests, in the same module
      CartRow.k
    catalog/
      Product.k              module shop.catalog
  testing/
    FakeCartApi.k            a test-only module
```

- A file has at most one `public` declaration, and the file is named after it: `CartService.k` holds
  `public service CartService`. You can always find a type from its name.
- Other declarations in a file are helpers, visible within the module.
- **Tests sit next to the code they test** as `Name.test.k`, in the same module, so they can test
  module-private code.
- **A directory named `testing`**, at any depth, is a test-only module for shared fakes and fixtures.
  Only tests can import it, and it never reaches a build.

[Modules and visibility](../language/modules.md) covers imports and visibility in detail.

## `migrations/`

If your app has a database, its migrations live here, one per file, named
`0001_create_accounts.k`, `0002_add_note_pinning.k` and so on. `k migrate new` generates them. They are
not a module: they can't import anything, and nothing can import them. See
[Persistence](../apps/persistence.md).

## Lockfiles

The `*.lock` files are generated by the toolchain and **committed**. Each one records a value that
something outside your codebase depends on, so that a change which would break it is caught at build
time. [Project files and lockfiles](../reference/project-files.md) describes each one.

## Ignored files

A project's `.gitignore` excludes:

- `dist/`, the build output;
- `.k/`, the toolchain's local state, such as the SQLite database `k dev` uses;
- `.env`, your local development configuration.

Commit a `.env.example` instead, documenting what `.env` may contain. Like every committed example file,
it holds placeholders, never working values (see
[Configuration and secrets](../apps/config.md#example-files-hold-placeholders)).

## Creating a project

> **Planned.** `k new hello` will create an app skeleton in `./hello`, and
> `k new --library github.com/acme/charts` a library package in `./charts`. `k new` isn't built yet;
> [Your first app](first-app.md#creating-a-project) shows the files to write by hand, and the apps in
> `examples/` are complete projects to start from.
