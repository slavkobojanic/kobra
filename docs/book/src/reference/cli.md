# The k command

Everything ships as one binary, `k`: compiler, test runner, development server, migration tool and server
runtime. Commands exit with `0` on success, `1` when they found problems, and `2` on a usage or internal
error.

## Projects

| Command | Purpose |
|---|---|
| `k new <name>` | create an app in a new directory `<name>/` (an existing one only if it is empty): `k.toml`, `App.k`, `.gitignore`, a start page with one test and a theme, all passing `k check`; `<name>` is lower case letters, digits and hyphens, starting with a letter |
| `k new --library <module path>` | create a library package, in a directory named after the last path segment (**Planned** k-new-library) |

## Development

| Command | Purpose |
|---|---|
| `k dev` | build the `.development` environment, serve it on port 8080 (`--port` to change), rebuild and restart on every change, run the job worker and load `.env` and `[dev]`, with no setup (a SQLite database in `.k/`, a mail catcher); a build with errors keeps the last good one serving; at a workspace root, every app, wired together |
| `k run` | the same as `k dev`, without rebuilding on change |
| `k run --release`, `k dev --release` | the same, serving the client `k build` ships (minified, with short class names and source maps) instead of the readable development client |
| `k run --inspect`, `k dev --inspect` | the same, with the server's debugger port open (9229, or `--inspect <port>`) for Chrome DevTools and VS Code; see [Debugging](../getting-started/debugging.md) |
| `k hash-password` | print the Argon2id PHC string of the password read from standard input, as `Password` and an admin hash's `@Matches` expect |
| `k test` | run every test: one browser and one server per test, a fresh database each; exit `0` when all passed, `1` on a failure or project errors |
| `k test --filter <text>` | run only the tests whose full name (`Group › description`) contains the text; repeatable; matching nothing is an error |
| `k test --watch` | run the tests, then re-run the affected ones on every save |
| `k test --color always\|never\|auto` | when to color the report |
| `k check [paths…]` | parse and check the projects at the paths (the current directory by default): types and every static rule, doc links, lockfile breaking-change detection and, for a program without errors, every test; exit `1` on any problem; no watch mode |
| `k check --no-tests` | the same without running the tests |
| `k check --format json` | the same, with one JSON object per diagnostic (code, span, message, quick fixes) on standard output |
| `k check --color always\|never\|auto` | when to color human-readable output |
| `k fmt` | format code canonically, removing redundant spellings such as `public` on members (**Planned** k-fmt) |
| `k routes` | print the route table: one row per URL and method (pages, endpoint handlers, `routes.lock` redirects) with parameters and guards |
| `k doc` | render documentation from `///` comments (**Planned** k-doc) |
| `k explain <code>` | describe a diagnostic code (**Planned** k-explain; see the [diagnostics reference](diagnostics.md)) |
| `k lsp` | run the language server on standard input and output, for editors ([Editor setup](../getting-started/editor-setup.md)) |

> **Planned** (k-fmt, doc-blocks). The design adds the formatting check and the type-checking of `k` blocks in doc comments
> to `k check`.

For end-to-end tests, `k run` and `k dev` read `K_DEV_FETCH_ORIGINS`: comma-separated `from=to` origin
pairs, such as `https://api.open-meteo.com=http://127.0.0.1:5301`. A request the server makes through
`HttpClient` to `from`, or to a URL below it, goes to `to` instead, so a test can answer a third-party
API from a local fake. A built `dist/app` never reads it.

## Building

| Command | Purpose |
|---|---|
| `k build` | build the `.production` environment into `dist/` (`--out <dir>` to change), and update `rpc.lock`, `routes.lock` and `schema.lock` |

The client `k build` writes is ready to ship as it is. Each app gets only the part of the runtime its
pages use; every JavaScript file is minified, with a source map next to it (`app.3f2a1b9c.js.map`) that
maps the minified code straight back to your `.k` lines (the server built with the app serves the maps
only when `K_PUBLIC_SOURCE_MAPS=true`; see [Debugging](../getting-started/debugging.md)); class names and theme variables are short
(`_3`, `--k7`); and every page's document preloads the files it needs, so the browser fetches them all
at once. `k dev` and `k run` serve readable code with the long names (`panel_k3x9a1`), which is easier
to debug; select elements in tests by role or text, not by class name.

## Workspaces

At the root of a [workspace](../apps/workspaces.md), the commands apply to every member; in a member, to
that member with the libraries it depends on.

| Command | At a workspace root |
|---|---|
| `k check` | check every member, reporting each problem once, and run the tests of each member without errors |
| `k test` | run every member's tests, each after a `== <name>` line (`--watch` only in a member) |
| `k build` | build each app into `dist/<name>/` and update its lockfiles; libraries build nothing |
| `k dev` | run every app, on `--port`, `--port + 1`, …, with each `remote` rule wired to the sibling that serves its protocol |

## Databases

| Command | Purpose |
|---|---|
| `k migrate new "description"` | write the next migration from the difference between entities and `schema.lock`, and update the lock |
| `k migrate new --empty "description"` | write an empty migration, for data changes |
| `k migrate status` | list the migrations the development database (`DATABASE_URL`, else `.k/<package>.db`) has applied and the pending ones |
| `k migrate apply` | apply the pending migrations to the development database |
| `k migrate new --rename <entity or field> "description"` | rename a table or column so it follows its entity's or field's current name, which is never inferred; marked `@Breaking("single instance")` |
| `k migrate new --drop-retired "description"` | drop columns and tables retired by earlier migrations |
| `k migrate rename <old> <new>` | record a renamed entity or field in `schema.lock`, keeping its table or column; no migration |

`k migrate new` refuses, writing nothing, a change it can't make safely (a required column without a
default, a changed type, a new enum case of a stored field, a removed entity) and explains each one.

## Packages

| Command | Purpose |
|---|---|
| `k add <module path>@<tag>` | add a git-based package (**Planned** packages) |
| `k remove <module path>` | remove a package (**Planned** packages) |

## Protobuf

| Command | Purpose |
|---|---|
| `k proto export` | emit `.proto` files from K protocols (**Planned** protobuf) |

## The built server

`k build` produces `dist/app`, which has commands of its own:

| Command | Purpose |
|---|---|
| `dist/app serve` | the default: validate configuration, migrate, serve HTTP and run the job worker |
| `dist/app migrate` | validate configuration and apply pending migrations, then exit |
| `dist/app config` | list every configuration variable the build reads, never printing secret values |
| `dist/app probe <url>` | a health check for images without `curl`: exit `0` on a 2xx answer within 3 seconds |
| `dist/app jobs list --dead` | list dead background jobs |
| `dist/app jobs retry <id>` | re-queue a dead job |

Configuration errors at startup exit with status `78`. See [Deployment](../apps/deployment.md).
