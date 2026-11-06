# The k command

Everything ships as one binary, `k`: compiler, test runner, development server, migration tool and server
runtime. Commands exit with `0` on success, `1` when they found problems, and `2` on a usage or internal
error.

## Projects

| Command | Purpose |
|---|---|
| `k new <name>` | create an app in a new directory `<name>/` |
| `k new --library <module path>` | create a library package, in a directory named after the last path segment |

## Development

| Command | Purpose |
|---|---|
| `k dev` | build the `.development` environment, serve it, rebuild on save, run the job worker and load `.env` |
| `k test` | run every test |
| `k test --watch` | run the tests, then re-run the affected ones on every save |
| `k check` | type checking, formatting, tests, doc-comment checks and lockfile breaking-change detection; fails on any problem; no watch mode |
| `k check --format json` | the same, with machine-readable diagnostics and quick fixes |
| `k fmt` | format code canonically, removing redundant spellings such as `public` on members (**Planned**) |
| `k routes` | print the route table, endpoint pages included |
| `k doc` | render documentation from `///` comments |

## Building

| Command | Purpose |
|---|---|
| `k build` | build the `.production` environment into `dist/` |

## Databases

| Command | Purpose |
|---|---|
| `k migrate new "description"` | write the next migration from the difference between entities and `schema.lock`, and update the lock |
| `k migrate new --empty "description"` | write an empty migration, for data changes |
| `k migrate new --rename …` | write a physical table or column rename, which is never inferred |
| `k migrate new --drop-retired "description"` | drop columns and tables retired by earlier migrations |

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
