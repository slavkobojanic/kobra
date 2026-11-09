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
| `k dev` | Dev server: builds `.development`, serves it and rebuilds and restarts on every change (hot module replacement is planned). Runs the background-job worker, loads `.env` and the project's `[dev]` settings, and needs no setup ([25-config-and-secrets.md](25-config-and-secrets.md#development-needs-no-setup)). At a workspace root it serves every app. |
| `k run` | `k dev` without watching: builds once and serves until Ctrl-C, every app at a workspace root. `k run --release` and `k dev --release` serve the client `k build` ships (minified, tree-shaken, short class names, source maps) with the development wiring |
| `k hash-password` | Prints the Argon2id PHC string of the password on standard input, with `k.crypto.Password`'s parameters, for scripts that make an admin password's hash |
| `k build` | Ahead-of-time build of the `.production` environment into separate server and client bundles |
| `k check` | Type checking, formatting check, tests, doc-comment checks, and lockfile breaking-change detection. Fails in CI on any problem. It has no watch mode. |
| `k fmt` | Canonical formatter |
| `k test`, `k test --watch` | Runs tests. `--watch` re-runs the affected tests on every save ([21-testing.md](21-testing.md)). |
| `k migrate new "description"` | Writes the next migration from the difference between the entities and `schema.lock` ([19-persistence.md](19-persistence.md)); `--empty`, `--drop-retired` and `--rename <name>` write the other kinds |
| `k migrate rename <old> <new>` | Records a renamed entity or field in `schema.lock`, keeping its table or column ([19-persistence.md](19-persistence.md#renames)) |
| `k routes` | Prints the route table, endpoint pages included |
| `k proto export` | Emits `.proto` files from K protocols |
| `k doc` | Renders documentation from `///` comments ([39-doc-comments.md](39-doc-comments.md)) |
| `k lsp` | The language server, over standard input and output, for editors ([docs/compiler/lsp.md](../compiler/lsp.md)) |

The built executable has its own commands (`serve`, `migrate`, `config`, `probe`, `jobs`), described in [38-deployment.md](38-deployment.md).

- **`k check` runs the tests** of a program without errors, lockfiles included, exactly as `k test` runs them, and reports each failure as a `K7510` diagnostic among the others; the summary counts the tests that ran. **`--no-tests`** skips them, for a quick type check. Only `k check` runs tests: the language server and its warm re-check never do, so editor latency doesn't grow with the test suite. CI runs `k check` alone; a separate `k test` step is unnecessary.
- **`k routes`** prints one row per URL and method: `METHOD`, `URL`, `PAGE` (an endpoint page or a `routes.lock` redirect says so), `PARAMS` (path parameters as `name: Type`, query parameters as `?name: Type = default`) and `GUARDS` (the `requires` clause as written), sorted by URL, then method.
- **`k migrate status` and `k migrate apply`** act on the development database (`DATABASE_URL` from the environment, `.env` or `[dev.env]`, else `sqlite:.k/<package>.db`, like `k dev`), so a developer can inspect and migrate it without starting the server.
- **`k hash-password`** reads one line, the password, and prints its hash; an empty password exits 2. It reads standard input so the password never appears in a process list. `examples/shop/scripts/init-secrets.sh` uses it, so making the Compose secrets needs neither the `argon2` tool nor Docker; without any way to hash, the script fails instead of writing an empty hash.
- **`k build --out dir` removes the hashed assets of earlier builds** from `dir/public/assets/`; nothing else in `dir` is touched.
- **`K_DEV_FETCH_ORIGINS` points a third party at a fake, in development only.** It holds comma-separated `from=to` origin pairs (`https://api.open-meteo.com=http://127.0.0.1:5301`); under `k run` and `k dev`, a server-side `HttpClient` request to `from`, or below it, goes to `to` instead. It exists so an app's end-to-end tests never reach a third party whose URL the app hard-codes, without making that URL configuration the app must carry: a config is server-only, and a service like the weather example's runs in the browser too, where the test intercepts its requests itself. Built apps (`dist/app`) never read it, so production can't be redirected by a stray variable.
- **Release builds are small by construction, with no tool outside `k`.** `k build` links the client: generated code imports the runtime by name, so each app's runtime chunk holds only what its chunks reach; every chunk is then minified in process by **oxc** (Rust, pinned to an exact version because its output is part of every content hash) with a source map per chunk; scoped class names and `--k-*` custom properties get short names; wire keys are quoted; and every document preloads its module graph (`<link rel="modulepreload">`). Development, `k run`, `k test` and the golden corpus keep readable output. **Closure Compiler was rejected**: it needs a JVM, adds about a second per app, and its property renaming broke a wire format in the measurement while gaining at most 6% over oxc. The numbers and per-app budgets are in [docs/compiler/performance.md](../compiler/performance.md) §11.7, the mechanics in [docs/compiler/codegen.md](../compiler/codegen.md) §17.
- **`--release` on `k run` and `k dev`** builds the client as `k build` does but keeps the development environment (`.env`, `[dev]`, the development database), so the shipped code can be tried, and the real-browser tests can run it, without a deployment's configuration.
- **The examples are tested in a real browser** as well as in the simulated tab: headless Chrome over the DevTools protocol, with real input events, layout, computed styles, fonts and focus, skipped with a message when no Chrome is installed (`K_REQUIRE_CHROME=1` in CI). A simulated DOM can't see a label nobody can read, a form that reloads the page or a theme that never reaches the document ([docs/compiler/testing.md](../compiler/testing.md) §4.4). They run each example's **release** client (`k build` for the static apps, `k run --release` / `k dev --release` for the others), since that is the code people get; `K_CHROME_PROFILE=development` runs the development client instead.

## Formatter

- **One canonical style, with no configuration options,** like `gofmt`. Agents and humans produce identical layouts, and diffs stay clean.
- One blank line between sibling view nodes (see [05-views.md](05-views.md)).
- It removes redundant spellings that have a machine-applicable fix, such as `public` on a member ([01-principles.md](01-principles.md#one-obvious-way)).
- `k check` fails on unformatted code.

## Lockfiles

All are committed to the repository and checked on every build:

- `routes.lock`: page URLs and redirects ([12-routing.md](12-routing.md#routeslock-and-redirects))
- `schema.lock`: database schema, migration order, and table and column names ([19-persistence.md](19-persistence.md#the-schemalock-file))
- `storage.lock`: the storage namespace, `StorageKey` names and value shapes, and job names and payload shapes ([36-local-storage.md](36-local-storage.md), [30-background-jobs.md](30-background-jobs.md))
- `rpc.lock`: the wire name, rpcs, field numbers and error cases of every protocol a serving app implements ([40-workspaces-and-services.md](40-workspaces-and-services.md#compatibility-between-deploys))
- `k.lock`: package versions and hashes ([18-packages.md](18-packages.md))

**`protocol.lock` is folded into `rpc.lock`.** `rpc.lock` records each served protocol's wire service name (`service = "shop.store.StoreProtocol"`: the Protobuf package from the project's name and the module, and the protocol's name), its rpcs, the Protobuf field numbers of the types they carry and their reserved numbers, so everything `protocol.lock` was to protect is compared on every check: a removed or renamed field, protocol or rpc, a changed type, a reused number and a changed wire name are all `K8001`. The one thing `protocol.lock` would have added, keeping the old wire names after a rename, is dropped: wire names are always derived ([28-rpc-wire-format.md](28-rpc-wire-format.md#names)), and a rename that changes one is a breaking change like any other, allowed with `@Breaking("reason")`.

**When lockfiles are written.** `k check` only reads them, so CI catches what a developer didn't build. `k build` writes `rpc.lock`, `routes.lock` and `schema.lock` (the schema the migrations produce) when they differ from what the program implies; `k migrate new` and `k migrate rename` write `schema.lock` with their change. Nothing writes `storage.lock` yet: its entries are compared once recorded, and when the compiler should write them is open.

**The lockfile checks that need the persistence model and the route table** (`schema.lock` and the migrations, `routes.lock`) run in `k check` beside the semantic pass, for an app without other errors. The language server doesn't run them; it reports the `rpc.lock` and `storage.lock` checks, which are part of the semantic pass.

## Language server

Features the design relies on:

- Hovering an effect shows its reactive dependencies.
- Each page's current URL is shown inline.
- Autocomplete after an implicit member (`.`) lists only valid members of the expected type, for example spacing tokens after `padding: .`.
- Autocomplete knows views and styles: the events after `.on(`, CSS properties, keyword values and theme tokens in style blocks, and the component's classes wherever a class goes.
- Quick fixes for structured compiler errors, such as changing `@Prop` to `@Binding` or `a` to `link`.

Decisions made with the server:

- **Rename keeps the program valid.** Renaming a public declaration renames its file (a file's public
  declaration must share its name); renaming an interface method renames the methods implementing it, and
  the reverse; renaming a page renames its cases (`.signIn` for `SignIn`); renaming a parameter whose name is
  also its label keeps the label (`a: Int` becomes `a b: Int`). Imports and doc links are references like any
  other. Standard library declarations can't be renamed.
- **Completion in `.on(…)` follows the one-event rule.** `.on(` offers the receiver's events (a component's
  `@Emits`, an element's events) as `edit:` labels, leaving out those another `.on(…)` of the same chain
  already handles. After a first event's comma (`.on(edit: { … }, `) it offers nothing: a call takes one
  event (K3108), so offering a second would suggest an error; the next event goes in a chained `.on(`.
- **Tokens are offered with their dot where none is typed.** After `gap: ` the list holds `.sm`, `.md`
  (inserting the dot) beside the keywords; after `gap: .` it holds `sm`, `md`. Each shows its value, so
  picking a token doesn't need a trip to the theme. Of two tokens of one name, the project theme's wins
  over the stdlib's, as in resolution.
- **`(` and `:` trigger completion only where they start something.** They open the list after `.on(` and
  in CSS values, and answer nothing elsewhere, so a call or a type annotation doesn't pop up every name in
  scope. Explicit invocation still lists the names in scope there.
- **Class completion is per place.** A class reference (`.class(.`, a `@Stylable` prop) offers the classes
  the style block declares, since anything else is K3203. A selector also offers the classes the view
  references without a rule yet, which is what the user is about to write.
- **Doc links are references.** Find references, rename and go-to-definition treat ``[`Name`]`` in a `///`
  comment like a use of `Name` in code, and hover renders it as a link.
- **Editors** ship in the repository (`editors/`): VS Code, Neovim and Vim. npm is used only to package the
  VS Code extension, never by the toolchain.
- **The TextMate grammar follows the lexer on strings and comments.** A string ends at its closing quote or at
  the end of its line (strings cannot span lines), an interpolation `{...}` balances the braces inside it
  (a trailing lambda, a `style {}` block), and `test "name"` leaves the name to the ordinary string rule. A
  `cargo test` (`k_lsp/tests/textmate.rs`) runs the grammar with a small TextMate engine and checks, over
  every lexer-clean `.k` file of the examples, the standard library and the test corpus, that each string
  token of the lexer is coloured as a string, each comment as a comment and every other token as code.
  An engine in Rust rather than `vscode-textmate` keeps npm out of the toolchain's tests.

## Compiler errors

Errors are structured and machine-readable, with quick fixes that agents can apply directly.
