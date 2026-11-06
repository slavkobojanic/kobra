# The k command

Everything ships as one binary, `k`. Commands exit with `0` on success, `1` when they found problems, and
`2` on a usage or internal error.

## Projects

| Command | Purpose |
|---|---|
| `k new <name>` | create an app in a new directory (*proposal*) |
| `k new --library <module path>` | create a library package (*proposal*) |

## Development

| Command | Purpose |
|---|---|
| `k dev` | build the `.development` environment, serve it, and rebuild on save |
| `k test` | run every test |
| `k test --watch` | re-run affected tests on save (*proposal* for the flag name) |
| `k check` | type checking, formatting, tests and lockfile breaking-change detection; fails on any problem |
| `k check --format json` | the same, with machine-readable diagnostics and quick fixes |
| `k routes` | print the route table |
| `k fmt` | format code canonically (**Planned**) |

## Building

| Command | Purpose |
|---|---|
| `k build` | build the `.production` environment into `dist/` |
| `k build --target edge` | build for edge platforms (**Planned**) |

## Databases

| Command | Purpose |
|---|---|
| `k migrate new "description"` | generate a migration from the difference between entities and `schema.lock` |
| `k migrate new --empty "description"` | generate an empty migration for data changes (*proposal*) |

## Packages

| Command | Purpose |
|---|---|
| `k add <module path>@<tag>` | add a git-based package |
| `k remove <module path>` | remove a package (*proposal*) |

## Protobuf

| Command | Purpose |
|---|---|
| `k proto export` | emit `.proto` files from K protocols (**Planned**) |

## The built server

`k build` produces `dist/app`, which has commands of its own (*proposal*):

| Command | Purpose |
|---|---|
| `dist/app serve` | the default: validate configuration, migrate, serve |
| `dist/app config` | list every configuration variable the build reads |
| `dist/app probe <url>` | a health check for images without `curl` |
