# The k command

Everything ships as one binary, `k`: compiler, test runner, development server, migration tool and server
runtime. Commands exit with `0` on success, `1` when they found problems, and `2` on a usage or internal
error.

## Projects

| Command | Purpose |
|---|---|
| `k new <name>` | create an app in a new directory `<name>/` (**Planned**) |
| `k new --library <module path>` | create a library package, in a directory named after the last path segment (**Planned**) |

## Development

| Command | Purpose |
|---|---|
| `k dev` | build the `.development` environment, serve it, rebuild on save, run the job worker and load `.env` |
| `k test` | run every test: one browser and one server per test, a fresh database each; exit `0` when all passed, `1` on a failure or project errors |
| `k test --filter <text>` | run only the tests whose full name (`Group › description`) contains the text; repeatable; matching nothing is an error |
| `k test --watch` | run the tests, then re-run the affected ones on every save |
| `k test --color always\|never\|auto` | when to color the report |
| `k check` | type checking, formatting, tests, doc-comment checks and lockfile breaking-change detection; fails on any problem; no watch mode |
| `k check --format json` | the same, with machine-readable diagnostics and quick fixes |
| `k fmt` | format code canonically, removing redundant spellings such as `public` on members (**Planned**) |
| `k routes` | print the route table: one row per URL and method (pages, endpoint handlers, `routes.lock` redirects) with parameters and guards |
| `k doc` | render documentation from `///` comments (**Planned**) |
| `k lsp` | run the language server on standard input and output, for editors ([Editor setup](../getting-started/editor-setup.md)) |

## Building

| Command | Purpose |
|---|---|
| `k build` | build the `.production` environment into `dist/`, and update `rpc.lock` |

## Workspaces

At the root of a [workspace](../apps/workspaces.md), the commands apply to every member; in a member, to
that member with the libraries it depends on.

| Command | At a workspace root |
|---|---|
| `k check` | check every member, reporting each problem once |
| `k test` | run every member's tests, each after a `== <name>` line (`--watch` only in a member) |
| `k build` | build each app into `dist/<name>/` and update its `rpc.lock`; libraries build nothing |
| `k dev` | run every app, on `--port`, `--port + 1`, …, with each `remote` rule wired to the sibling that serves its protocol |

## Databases

| Command | Purpose |
|---|---|
| `k migrate new "description"` | write the next migration from the difference between entities and `schema.lock`, and update the lock |
| `k migrate new --empty "description"` | write an empty migration, for data changes |
| `k migrate status` | list the migrations the development database (`DATABASE_URL`) has applied and the pending ones |
| `k migrate apply` | apply the pending migrations to the development database |
| `k migrate new --rename …` | write a physical table or column rename, which is never inferred (**Planned**) |
| `k migrate new --drop-retired "description"` | drop columns and tables retired by earlier migrations (**Planned**) |

`k migrate new` refuses, writing nothing, a change it can't make safely (a required column without a
default, a changed type, a new enum case of a stored field) and explains each one.

## Packages

| Command | Purpose |
|---|---|
| `k add <module path>@<tag>` | add a git-based package (**Planned**) |
| `k remove <module path>` | remove a package (**Planned**) |

## Protobuf

| Command | Purpose |
|---|---|
| `k proto export` | emit `.proto` files from K protocols (**Planned**) |

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
