# Tooling

## Implementation language

**The toolchain is written in Rust,** with a query-based incremental architecture like rust-analyzer's (salsa).

- **The compiler and the language server are the same engine.** Every IDE feature reuses compiler queries, so the IDE never disagrees with the compiler.
- **Edits recompute only what they affect,** which makes state-preserving HMR and instant diagnostics practical on large codebases.
- **Everything ships as one static `k` binary:** compiler, language server, formatter, dev server and test runner. Installing K needs no Node tooling.

## Server runtime

**Server code runs on a JavaScript runtime embedded in the `k` binary** (V8, through `deno_core`). No Node, Bun or npm is involved.

- **`k build` produces `dist/app`, a single self-contained server executable,** plus hashed client assets. Deploying means copying one file, or using a minimal container. An app with no server-only code builds to static files only. See [38-deployment.md](38-deployment.md).
- **The runtime exposes only web-standard APIs** (fetch, streams, Web Crypto). The stdlib is built on them.
- **Development and production use the same runtime.**
- **Later:** `k build --target edge` for Cloudflare Workers and similar platforms. The compiler would reject edge-incompatible features (long-running jobs, direct Postgres sockets) at compile time.

## Commands

| Command | Purpose |
|---|---|
| `k new <name>`, `k new --library <module path>` | Creates a project from the fixed skeleton ([24-projects.md](24-projects.md)) |
| `k dev` | Dev server with state-preserving hot module replacement. Runs the background-job worker and loads `.env` ([25-config-and-secrets.md](25-config-and-secrets.md)). |
| `k build` | Ahead-of-time build of the `.production` environment into separate server and client bundles |
| `k check` | Type checking, formatting check, tests, doc-comment checks, and lockfile breaking-change detection. Fails in CI on any problem. It has no watch mode. |
| `k fmt` | Canonical formatter |
| `k test`, `k test --watch` | Runs tests. `--watch` re-runs the affected tests on every save ([21-testing.md](21-testing.md)). |
| `k migrate new "description"` | Writes the next migration from the difference between the entities and `schema.lock` ([19-persistence.md](19-persistence.md)) |
| `k routes` | Prints the route table, endpoint pages included |
| `k proto export` | Emits `.proto` files from K protocols |
| `k doc` | Renders documentation from `///` comments ([39-doc-comments.md](39-doc-comments.md)) |
| `k lsp` | The language server, over standard input and output, for editors ([docs/compiler/lsp.md](../compiler/lsp.md)) |

The built executable has its own commands (`serve`, `migrate`, `config`, `probe`, `jobs`), described in [38-deployment.md](38-deployment.md).

- **`k routes`** prints one row per URL and method: `METHOD`, `URL`, `PAGE` (an endpoint page or a `routes.lock` redirect says so), `PARAMS` (path parameters as `name: Type`, query parameters as `?name: Type = default`) and `GUARDS` (the `requires` clause as written), sorted by URL, then method.
- **`k migrate status` and `k migrate apply`** act on the development database (`DATABASE_URL` from the environment or `.env`, like `k dev`), so a developer can inspect and migrate it without starting the server.
- **`k build --out dir` removes the hashed assets of earlier builds** from `dir/public/assets/`; nothing else in `dir` is touched.

## Formatter

- **One canonical style, with no configuration options,** like `gofmt`. Agents and humans produce identical layouts, and diffs stay clean.
- One blank line between sibling view nodes (see [05-views.md](05-views.md)).
- It removes redundant spellings that have a machine-applicable fix, such as `public` on a member ([01-principles.md](01-principles.md#one-obvious-way)).
- `k check` fails on unformatted code.

## Lockfiles

All are committed to the repository and checked on every build:

- `protocol.lock`: Protobuf package, service and method names and field numbers ([11-protocols-and-rpc.md](11-protocols-and-rpc.md), [28-rpc-wire-format.md](28-rpc-wire-format.md))
- `routes.lock`: page URLs and redirects ([12-routing.md](12-routing.md))
- `schema.lock`: database schema, migration order, and table and column names ([19-persistence.md](19-persistence.md))
- `storage.lock`: the storage namespace, `StorageKey` names and value shapes, and job names and payload shapes ([36-local-storage.md](36-local-storage.md), [30-background-jobs.md](30-background-jobs.md))
- `k.lock`: package versions and hashes ([18-packages.md](18-packages.md))

When each lockfile is first written, and whether `k build` updates them or only an explicit command does, is still open.

## Language server

Features the design relies on:

- Hovering an effect shows its reactive dependencies.
- Each page's current URL is shown inline.
- Autocomplete after an implicit member (`.`) lists only valid members of the expected type, for example spacing tokens after `padding: .`.
- Quick fixes for structured compiler errors, such as changing `@Prop` to `@Binding` or `a` to `link`.

Decisions made with the server:

- **Rename keeps the program valid.** Renaming a public declaration renames its file (a file's public
  declaration must share its name); renaming an interface method renames the methods implementing it, and
  the reverse; renaming a page renames its cases (`.signIn` for `SignIn`); renaming a parameter whose name is
  also its label keeps the label (`a: Int` becomes `a b: Int`). Imports and doc links are references like any
  other. Standard library declarations can't be renamed.
- **Doc links are references.** Find references, rename and go-to-definition treat ``[`Name`]`` in a `///`
  comment like a use of `Name` in code, and hover renders it as a link.
- **Editors** ship in the repository (`editors/`): VS Code, Neovim and Vim. npm is used only to package the
  VS Code extension, never by the toolchain.

## Compiler errors

Errors are structured and machine-readable, with quick fixes that agents can apply directly.
