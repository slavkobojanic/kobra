# Changelog

Every released version of Kobra, newest first. Each section is compiled from the changelog fragments in
`changes/` when the version is prepared (`cargo xtask release prepare`, docs/compiler/release.md).
The book's changelog page (`docs/book/src/reference/changelog.md`) is a copy.

## 0.1.0-rc.1 (pre-release of 0.1.0, 2026-11-22)

### Highlights

- The first release of Kobra: one `k` binary that checks, tests, formats, runs and builds full-stack web
  apps, and a language server for editors. Nothing to migrate.
- `dist/app` is one self-contained server executable per app, for Docker, Compose and Kubernetes.
- A project pins its toolchain from now on: `k = "0.1"` in `k.toml`, the exact version in `k.lock`; a
  newer `k` hands the command to the pinned one, and `k upgrade` moves a project to a later minor.
- This release's wire formats (rpc encoding, service tokens, session cookies, job payloads) are
  recorded: 0.2 reads them, so a rolling deploy from 0.1 to 0.2 works, one minor at a time.
- Collaborators install from the private repository with the GitHub CLI: `gh auth login`, then
  `gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' | sh`
  (or `install.ps1` on Windows), or `k toolchain install`; every download is checked against its
  SHA-256, and `k docs` opens the book installed with the toolchain. Kobra is MIT OR Apache-2.0.

### Language

- Components with a `view` of built-in elements, fine-grained reactivity without a virtual DOM, typed styles and themes compiled to CSS, element references, and `@Rendering(.client)` for components that render only in the browser. See [Components](docs/book/src/apps/components.md), [Views and built-in elements](docs/book/src/apps/views.md), [Reactivity](docs/book/src/apps/reactivity.md), [Styles and themes](docs/book/src/apps/styles.md).
- Kobra, a statically typed language that compiles to JavaScript: structs, enums with payloads, classes and interfaces, generics with explicit type arguments and read-only covariance, units, typed errors with `throws`, `catch` by type and `catch unknown`, `async` functions and `Stream`s, and modules with visibility checked at compile time. See [The basics](docs/book/src/language/basics.md), [Types](docs/book/src/language/types.md), [Structs, enums, classes and interfaces](docs/book/src/language/declarations.md), [Errors](docs/book/src/language/errors.md), [Modules and visibility](docs/book/src/language/modules.md).

### Standard library

- Accounts and sessions with passwords and passkeys, `k.crypto` (keys, signatures, sealed values, JWTs) on the server and in the browser, and typed configuration with secrets read and validated at startup. See [Authentication and sessions](docs/book/src/apps/auth.md), [Bytes and cryptography](docs/book/src/apps/crypto.md), [Configuration and secrets](docs/book/src/apps/config.md).
- Entities and repositories over SQLite and Postgres with migrations generated from `schema.lock`, background and scheduled jobs, email, a typed HTTP client, and the browser's local storage and indexed collections. See [Persistence](docs/book/src/apps/persistence.md), [Background jobs](docs/book/src/apps/jobs.md), [Email](docs/book/src/apps/email.md), [Calling HTTP APIs](docs/book/src/apps/http.md), [Local storage](docs/book/src/apps/local-storage.md).
- Pages and routing with typed parameters and guards enforced on the server and in the browser, async data loading owned by its page, and forms with validation checked at compile time. See [Pages and routing](docs/book/src/apps/routing.md), [Async data loading](docs/book/src/apps/async-data.md), [Forms and validation](docs/book/src/apps/forms.md).
- The web platform as typed Kobra APIs: clipboard and sharing, location, notifications and Web Push, camera, microphone, recording and video calls, offline and installable apps, drag and drop, files, keyboard shortcuts, tabs and the screen, plus charts, money and time. See [Web platform APIs](docs/book/src/reference/web-platform.md), [Camera, microphone, recording and calls](docs/book/src/apps/media.md), [Offline and installable apps](docs/book/src/apps/offline.md), [Notifications](docs/book/src/apps/notifications.md), [Drag and drop](docs/book/src/apps/drag-and-drop.md), [Charts](docs/book/src/apps/charts.md), [Money](docs/book/src/apps/money.md), [Time and timers](docs/book/src/apps/time.md).

### Runtime

- `k build` makes one self-contained server executable, `dist/app`, with a minified, tree-shaken client and source maps that map back to your `.k` lines; it renders pages on the server and hydrates them, and runs under Docker, Compose and Kubernetes without other files. See [Deployment](docs/book/src/apps/deployment.md), [Debugging](docs/book/src/getting-started/debugging.md).
- Services with dependency injection checked per environment, protocols whose rpcs are called like functions with their shapes recorded in `rpc.lock`, server streams, WebSocket channels with presence across replicas, and workspaces whose apps call each other's services with service tokens. See [Services and dependency injection](docs/book/src/apps/services.md), [Protocols and RPC](docs/book/src/apps/protocols.md), [Server streaming](docs/book/src/apps/streaming.md), [Channels](docs/book/src/apps/channels.md), [Workspaces and services](docs/book/src/apps/workspaces.md).

### Tooling

- `editors/claude` is a Claude Code plugin with three skills (`kobra-language`, `kobra-apps` and `kobra-tooling`) that teach Claude to write idiomatic Kobra and to check it with the compiler. The plugin also starts the `k mcp` server, and installs with `/plugin marketplace add slavkobojanic/kobra` followed by `/plugin install kobra@kobra`. See [Editor setup](docs/book/src/getting-started/editor-setup.md).
- The VS Code extension shows the toolchain the language server runs (`K 0.1.0`) in its language status item, as the server reports it in `initialize`. See [Editor setup](docs/book/src/getting-started/editor-setup.md).
- `k fmt` formats Kobra code into its one canonical layout: the project or workspace, the paths named, or standard input (`-`); `--check` prints a diff and fails on unformatted files, and `--format json` reports each file. The language server formats documents and selections with it, and the VS Code extension formats on save. See [Everyday commands](docs/book/src/getting-started/commands.md), [The k command](docs/book/src/reference/cli.md), [Editor setup](docs/book/src/getting-started/editor-setup.md).
- The language is called Kobra (the command stays `k`, and source files `.k`), and it is dual-licensed MIT OR Apache-2.0, with both licences in every release archive. Until a stable release exists, the install scripts install the newest release candidate and say so. The VS Code extension is `slavkobojanic.kobra`, and the toolchain image `ghcr.io/slavkobojanic/kobra`. See [Installation](docs/book/src/getting-started/installation.md), [Quick start](docs/book/src/learn/quick-start.md), [Editor setup](docs/book/src/getting-started/editor-setup.md).
- `k mcp` is an MCP server for AI assistants (`claude mcp add kobra -- k mcp`): it serves the book, the design and compiler docs and the example apps built into `k`, plus the project it runs in, with tools to search and read them, explain a diagnostic code, and check Kobra code with the compiler. See [AI assistants (MCP)](docs/book/src/getting-started/ai-assistants.md), [The k command](docs/book/src/reference/cli.md).
- Kobra installs from its private GitHub repository for the people who have access to it: sign in with `gh auth login`, then run `gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' | sh` (on Windows, the same with `install.ps1 … | Out-String | iex`). `k toolchain install` and `k self update` download with `GH_TOKEN`, `GITHUB_TOKEN` or `gh auth token`, sent to GitHub's API only, and each toolchain comes with its book, which the new `k docs` opens (`k docs --path` prints where it is). See [Installation](docs/book/src/getting-started/installation.md), [Quick start](docs/book/src/learn/quick-start.md), [The k command](docs/book/src/reference/cli.md).
- One `k` binary: `k new`, `k check`, `k fix`, `k test` (a browser and a server per test), `k dev`, `k build`, `k routes`, `k migrate` and `k lsp`, the language server behind the VS Code extension. See [Everyday commands](docs/book/src/getting-started/commands.md), [The k command](docs/book/src/reference/cli.md), [Editor setup](docs/book/src/getting-started/editor-setup.md), [Testing](docs/book/src/apps/testing.md).
- `k toolchain install`, `list`, `remove` and `default` manage installed toolchains, and `k self update` installs the newest release and makes it the default; downloads are checked against the index's size and SHA-256, and an interrupted install leaves nothing behind. `install.sh` and `install.ps1` install Kobra into `~/.k` in one line. See [The k command](docs/book/src/reference/cli.md), [Installation](docs/book/src/getting-started/installation.md), [Upgrading Kobra](docs/book/src/apps/upgrading.md).
- A project pins its toolchain: `k = "0.1"` in `k.toml` and the exact version in `k.lock`, with every command passed to the matching installed toolchain, `K_TOOLCHAIN` for one command, a format version in every lockfile, `@Deprecated` with machine-applicable fixes, and `k upgrade` to move a project to a newer toolchain. See [The k command](docs/book/src/reference/cli.md), [Project files and lockfiles](docs/book/src/reference/project-files.md), [Upgrading Kobra](docs/book/src/apps/upgrading.md).

### Preview features

- `@Worker` services run off the main thread in Web Workers, behind the `workers` preview: list it in `[package] preview` to use them. See [Work off the main thread](docs/book/src/apps/workers.md).

### Fixes

- The test fakes now write a `Double` as a JSON number. `TestHttpClient.serveJson`, `MemoryStorage`, the in-memory collections and `RecordingJobQueue` used to encode it as an object, so decoding the value back failed with `expected a number`.

### Documentation

- The Kobra book: a quick start, the Shelf tutorial in ten steps, guides for every part of the language and the standard library, nine example apps, and references generated from the compiler, every sample compiled against it.
