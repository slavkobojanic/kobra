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
| `k run` | `k dev` without watching: builds once and serves until Ctrl-C, every app at a workspace root. `k run --release` and `k dev --release` serve the client `k build` ships (minified, tree-shaken, short class names, source maps) with the development wiring; `--inspect[=port]` opens the server's V8 inspector ([Debugging](#debugging)) |
| `k hash-password` | Prints the Argon2id PHC string of the password on standard input, with `k.crypto.Password`'s parameters, for scripts that make an admin password's hash |
| `k build` | Ahead-of-time build of the `.production` environment into separate server and client bundles |
| `k check` | Type checking, formatting check, tests, doc-comment checks, and lockfile breaking-change detection. Fails in CI on any problem. It has no watch mode. |
| `k fix` | Applies the machine-applicable fixes of the current diagnostics and checks again until none is left, then reports what remains; `--code K1234` limits it to one code, `--dry-run` prints the diff ([48-releases-and-versioning.md](48-releases-and-versioning.md#upgrading-a-project)) |
| `k fmt` | Canonical formatter |
| `k test`, `k test --watch` | Runs tests. `--watch` re-runs the affected tests on every save ([21-testing.md](21-testing.md)). |
| `k migrate new "description"` | Writes the next migration from the difference between the entities and `schema.lock` ([19-persistence.md](19-persistence.md)); `--empty`, `--drop-retired` and `--rename <name>` write the other kinds |
| `k migrate rename <old> <new>` | Records a renamed entity or field in `schema.lock`, keeping its table or column ([19-persistence.md](19-persistence.md#renames)) |
| `k routes` | Prints the route table, endpoint pages included |
| `k proto export` | Emits `.proto` files from K protocols |
| `k doc` | Renders documentation from `///` comments ([39-doc-comments.md](39-doc-comments.md)) |
| `k lsp` | The language server, over standard input and output, for editors ([docs/compiler/lsp.md](../compiler/lsp.md)) |
| `k toolchain install`, `list`, `remove`, `default`; `k self update` | Install, list, remove and choose the default of the installed toolchains, and update the default to the newest release; the only commands that download ([48-releases-and-versioning.md](48-releases-and-versioning.md#commands)) |

The built executable has its own commands (`serve`, `migrate`, `config`, `probe`, `jobs`), described in [38-deployment.md](38-deployment.md).

- **`k check` runs the tests** of a program without errors, lockfiles included, exactly as `k test` runs them, and reports each failure as a `K7510` diagnostic among the others; the summary counts the tests that ran. **`--no-tests`** skips them, for a quick type check. Only `k check` runs tests: the language server and its warm re-check never do, so editor latency doesn't grow with the test suite. CI runs `k check` alone; a separate `k test` step is unnecessary.
- **`k routes`** prints one row per URL and method: `METHOD`, `URL`, `PAGE` (an endpoint page or a `routes.lock` redirect says so), `PARAMS` (path parameters as `name: Type`, query parameters as `?name: Type = default`) and `GUARDS` (the `requires` clause as written), sorted by URL, then method; then the channel rpcs, as `WS` rows at their wire URL with `CHANNEL (Protocol.rpc)` as the page ([44-channels.md](44-channels.md#visibility-in-development-and-production)).
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
- `storage.lock`: the storage namespace, `StorageKey` names and value shapes, indexed collections, and job names and payload shapes ([36-local-storage.md](36-local-storage.md), [52-indexed-collections.md](52-indexed-collections.md), [30-background-jobs.md](30-background-jobs.md))
- `rpc.lock`: the wire name, rpcs, field numbers and error cases of every protocol a serving app implements ([40-workspaces-and-services.md](40-workspaces-and-services.md#compatibility-between-deploys))
- `k.lock`: the exact toolchain that last built the app or workspace ([48-releases-and-versioning.md](48-releases-and-versioning.md#the-requirement-and-the-lock)), and later package versions and hashes ([18-packages.md](18-packages.md))

**`protocol.lock` is folded into `rpc.lock`.** `rpc.lock` records each served protocol's wire service name (`service = "shop.store.StoreProtocol"`: the Protobuf package from the project's name and the module, and the protocol's name), its rpcs, the Protobuf field numbers of the types they carry and their reserved numbers, so everything `protocol.lock` was to protect is compared on every check: a removed or renamed field, protocol or rpc, a changed type, a reused number and a changed wire name are all `K8001`. The one thing `protocol.lock` would have added, keeping the old wire names after a rename, is dropped: wire names are always derived ([28-rpc-wire-format.md](28-rpc-wire-format.md#names)), and a rename that changes one is a breaking change like any other, allowed with `@Breaking("reason")`.

**When lockfiles are written.** `k check` only reads them, so CI catches what a developer didn't build. `k build` writes `rpc.lock`, `routes.lock` and `schema.lock` (the schema the migrations produce) when they differ from what the program implies; `k migrate new` and `k migrate rename` write `schema.lock` with their change. `k build` also writes `k.lock`'s `toolchain` when another allowed toolchain built the project. `k build` writes `storage.lock` too, when it differs from what the program implies ([52-indexed-collections.md](52-indexed-collections.md#storagelock)).

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

## Debugging

**K is debugged as K, in the browser's DevTools and in VS Code, with no tool outside `k`.** Breakpoints
are set in `.k` files, the paused line is the `.k` line, stack traces name `.k` lines, and the values in
the Scope pane and the console read as K values. The mechanics are in
[docs/compiler/codegen.md §18](../compiler/codegen.md#18-source-maps) (maps) and
[docs/compiler/server.md §19](../compiler/server.md#19-debugging-the-server) (the server's inspector); the
user guide is the book's Debugging chapter.

- **Source maps from the printer, not from text.** Lowering marks the JS IR with the K location it came
  from (`Stmt::At`, `Expr::At`: every lowered statement, the last expression of a block or lambda, view
  bindings, event handlers, `@Derived` and `@Load` initializers, component fields, style rules and
  declarations), and the printer records a mark where each starts. Every JavaScript chunk, every server
  module that holds K code and the development stylesheet get a v3 map built from those marks.
  **Decision:** line-and-statement granularity, plus the start of each binding and handler, is what
  breakpoints and stack traces need; per-identifier mappings would double the maps for no visible gain.
- **One virtual root for K sources.** **Decision:** a client map names `/_k/src/<project path>`
  (`/_k/src/src/home/Home.k`), a server module's map `k:/_k/src/<project path>`. Rejected `k://`: Chrome
  resolves a scheme it doesn't know as an opaque URL it never fetches, and VS Code's path overrides
  match prefixes, which `/_k/src/` keeps the same on both sides. `k run` and `k dev` serve the `.k` files
  at `/_k/src/`; development maps also embed them (`sourcesContent`), so a debugger needs neither.
  Stdlib K code maps below `/_k/src/$std/` and is on the `ignoreList`.
- **Release maps compose.** `k build` minifies with oxc, which maps minified code to the readable chunk;
  the linker composes that map with the chunk's K map, so minified code maps straight to `.k` lines and
  names. **Decision on publishing:** release maps are always written next to their chunks in
  `dist/public/assets` (`stem.<hash>.js.map`, referenced by the chunk), without the K text. `dist/app`
  serves them only when `ServerConfig.publicSourceMaps` is `true` (`K_PUBLIC_SOURCE_MAPS`, a `Bool`, default
  `false`; [25-config-and-secrets.md](25-config-and-secrets.md)): they reveal the code's structure and file names, so
  they are opt-in in production, while a team that wants production debugging opts in or loads the map
  files into DevTools by hand. A static host serves whatever `dist/public` holds: delete the `*.map` files
  before uploading to keep them private. The server's modules carry their maps inline and always map
  the server's logged stack traces. Symbolicating stack traces the browser reports to the server is
  deferred (there is no client error report yet).
- **Original names, not scopes.** Generated code keeps K's local names (`count`, `next`, `greeter`);
  where it can't (a JavaScript reserved word, `arguments` → `arguments$`; a shadowed name, `item$1`) the
  map's `names` gives the K name at the binding and its uses, and release maps carry the K name through
  the minifier's renaming. Chrome's Scope pane renames from these. **Decision:** no ECMA-426 `scopes`
  yet. The Scopes proposal is at Stage 3 of TC39-TG4 and the Chrome DevTools team publishes a codec
  for it, but we found no browser release that applies it by default (checked November 2026), and its
  encoding was still being revised; Firefox and Safari read only `names`. Generated names that remain
  (`$c`, `$life`, `$root3`) are the runtime's plumbing and keep their `$` so they read as such. Revisit
  when a browser ships Scopes by default.
- **The runtime is ignore-listed.** The runtime chunk, the feature chunks and the `app` entry (glue only)
  get a map whose one source is on the `ignoreList` (and Chrome's older `x_google_ignoreList`), so
  stepping skips them and stack traces collapse their frames; under `--inspect` the server's runtime
  modules carry the same. Async stack traces through `await` and rpc calls are V8's own and read as K
  once the runtime's frames are hidden.
- **Custom formatters, development only.** A development client appends `runtime/devtools.js` to its
  runtime chunk: a `devtoolsFormatters` entry that shows a `@State` or `@Derived` value as its current
  value (read without subscribing), `nil`, enum cases by name with their payloads, structs by field,
  `Money`, `Decimal` and `Date` as text, `List`, `Map` and `Set` with their sizes. Payload enum cases are
  named through a non-enumerable property only development clients set; payload-free cases are shared
  singletons across enums and show as `.#tag`. **Release and test builds contain none of it.**
- **The server's inspector.** **Decision:** `k run --inspect[=port]` and `k dev --inspect[=port]` (9229
  by default) rather than a new `k debug` subcommand (`k debug` prints compiler internals). The server
  then runs one worker isolate with V8's inspector on, served the way Node serves it (`/json/list`, a
  WebSocket per session), so `chrome://inspect` and VS Code's `node` attach work unchanged. In
  development a panic's 500 page and an rpc's `internal` error carry the stack trace mapped to `.k`
  lines; every server log line with a stack is mapped the same way.
- **VS Code without npm.** `editors/vscode/launch.json` holds launch configurations for VS Code's
  built-in JavaScript debugger: the browser (`chrome`, with `/_k/src/*` mapped to the workspace) and the
  server (`node` attach to the inspector, `k:/_k/src/*` mapped likewise), and a compound of both.
- **Tested in a real browser.** `k_codegen/tests/sourcemaps.rs` decodes the maps with the `sourcemap`
  crate and checks exact lines and columns; `k_cli/tests/debugging.rs` sets breakpoints at `.k` lines in
  headless Chrome and in the server's inspector, triggers them, and checks the paused `.k` line, the K
  names in scope, a mapped exception, the formatters and the mapped server traces.

## Compiler errors

Errors are structured and machine-readable, with quick fixes that agents can apply directly.

## The toolchain's own tests

**The suite runs under cargo-nextest, through `scripts/test.sh`, and agents run only the affected tests**
(`scripts/affected-tests.sh`). Details and measurements are in
[docs/compiler/testing.md §2.1–2.4](../compiler/testing.md#21-running-tests-fast).

- **nextest, pinned** (`cargo install cargo-nextest --version 0.9.111 --locked`; `.config/nextest.toml` names
  the minimum). Every test is a process and every binary runs at once, so the run takes as long as the slowest
  lane, not the sum of the binaries run one after another. Hangs fail after 2 minutes (5 in the `ci`
  profile); there are no retries.
- **Three suites stay in-process** (the golden corpus, the runtime's JS tests, the execution corpus): they have
  thousands of tiny cases, and a process per case would cost ~900 CPU-seconds against 6 s. nextest's default
  filter leaves them out; `scripts/test.sh` runs their binaries directly next to nextest, and the doc tests
  next to both.
- **Heavy tests are grouped and capped**: Chrome tests at 4 at once, tests that start `k` processes at 8. The
  slowest tests start first.
- **Dependencies are optimized in the dev profile** (`opt-level = 2`) and the workspace's crates lightly
  (`opt-level = 1`), with full debug info. Unoptimized crypto and SQLite dominated the server and CLI tests;
  `opt-level = 1` for the workspace halved the run for no slower rebuilds. A faster linker (lld) and less debug
  info were measured and gained nothing.
- **Slow tests are fixed in place, never weakened**: independent work inside a test runs on threads (the warm
  re-check across the examples, the corpus robustness sweep), ports are reserved across processes, and
  `QUICK=1` lowers property-test case counts for local iteration only (CI refuses it).
- **Affected tests are chosen from the change**: a crate's source selects it and its reverse dependencies from
  `cargo metadata`; a test file selects its module of the crate's `it` binary (and a support file the
  modules that use it); `examples/`, `docs/`, `editors/` and `tests/<dir>/` select
  the tests whose code names that directory in a string literal. The script prints each choice and its reason.
- **One target directory per concurrent agent.** Cargo builds one command at a time per target directory;
  agents sharing `target/` spent most of a 37-minute run waiting on each other's builds.
- **One test binary per crate** (`it`, rooted at `crates/<crate>/tests/main.rs`, `autotests = false`): each
  former test file is a module of it, so tests are named `<file>::<test>` and run with `cargo test -p <crate>
  --test it -- <file>::` or nextest's `binary_id(=<crate>::it) & test(/^<file>::/)`. The files stay where they
  were, so paths in the docs, `#[path]` includes and proptest regression files are unchanged; shared support
  modules are declared once in `tests/main.rs`. Only the suites with their own harness (`harness = false`: the
  golden corpus, the runtime's JS tests, the execution corpus, the e2e scripts) keep binaries of their own.
  This took the workspace from 87 integration-test executables to 14 and their size from 2.3 GB to 0.7 GB,
  with the same 5647 tests (and 46 doc tests) and the same run time (the run was already as long as its slowest lane); it saves
  links, macOS's first-run checks and disk. `scripts/test.sh` refuses to run when a `tests/*.rs` file is in
  no binary. Chosen over `tests/it/*.rs` (moving the files) because nothing outside `Cargo.toml` and
  `tests/main.rs` had to change paths.
