# Tooling

## Implementation language

**The toolchain is written in Rust,** with a query-based incremental architecture like rust-analyzer's (salsa).

- **The compiler and the language server are the same engine.** Every IDE feature reuses compiler queries, so the IDE never disagrees with the compiler.
- **Edits recompute only what they affect,** which makes state-preserving HMR and instant diagnostics practical on large codebases.
- **Everything ships as one static `k` binary:** compiler, language server, formatter, dev server and test runner. Installing K needs no Node tooling.

## Server runtime

**Server code runs on a JavaScript runtime embedded in the `k` binary** (V8, through `deno_core`). No Node, Bun or npm is involved.

- **`k build` produces `dist/app`, a single self-contained server executable,** plus hashed client assets. Deploying means copying one file, or using a minimal container.
- **The runtime exposes only web-standard APIs** (fetch, streams, Web Crypto). The stdlib is built on them.
- **Development and production use the same runtime.**
- **Later:** `k build --target edge` for Cloudflare Workers and similar platforms. The compiler would reject edge-incompatible features (long-running jobs, direct Postgres sockets) at compile time.

## Commands

| Command | Purpose |
|---|---|
| `k dev` | Dev server with state-preserving hot module replacement |
| `k build` | Ahead-of-time build into separate server and client bundles |
| `k check` | Type checking, formatting check, tests, and lockfile breaking-change detection. Fails in CI on any problem. |
| `k fmt` | Canonical formatter |
| `k test` | Runs tests, with a watch mode ([21-testing.md](21-testing.md)) |
| `k routes` | Prints the route table |
| `k proto export` | Emits `.proto` files from K protocols |

## Formatter

- **One canonical style, with no configuration options,** like `gofmt`. Agents and humans produce identical layouts, and diffs stay clean.
- One blank line between sibling view nodes (see [05-views.md](05-views.md)).
- `k check` fails on unformatted code.

## Lockfiles

Both are committed to the repository and checked on every build:

- `protocol.lock`: Protobuf field numbers ([11-protocols-and-rpc.md](11-protocols-and-rpc.md))
- `routes.lock`: page URLs and redirects ([12-routing.md](12-routing.md))
- `schema.lock`: database schema ([19-persistence.md](19-persistence.md))
- `k.lock`: package versions and hashes ([18-packages.md](18-packages.md))

## Language server

Features the design relies on:

- Hovering an effect shows its reactive dependencies.
- Each page's current URL is shown inline.
- Autocomplete after an implicit member (`.`) lists only valid members of the expected type, for example spacing tokens after `padding: .`.
- Quick fixes for structured compiler errors, such as changing `@Prop` to `@Binding` or `a` to `link`.

## Compiler errors

Errors are structured and machine-readable, with quick fixes that agents can apply directly.
