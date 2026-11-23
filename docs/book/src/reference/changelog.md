# Changelog

Every released version of Kobra, newest first: what changed, and for each breaking change and deprecation,
how to migrate. [Upgrading Kobra](../apps/upgrading.md) collects the migrations by minor. The page is
compiled from the changelog fragments each change adds, when the version is released, and is the same
as the repository's `CHANGELOG.md`.

<!-- gen:changelog -->
## 0.1.0-rc.2 (pre-release of 0.1.0, 2026-11-23)

### Highlights

- The first release of Kobra: one `k` binary that checks, tests, formats, runs and builds full-stack web
  apps, and a language server for editors. Nothing to migrate.
- `dist/app` is one self-contained server executable per app, for Docker, Compose and Kubernetes.
- A project pins its toolchain from now on: `k = "0.1"` in `k.toml`, the exact version in `k.lock`; a
  newer `k` hands the command to the pinned one, and `k upgrade` moves a project to a later minor.
- This release's wire formats (rpc encoding, service tokens, session cookies, job payloads) are
  recorded: 0.2 reads them, so a rolling deploy from 0.1 to 0.2 works, one minor at a time.
- Install with `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh` (or
  `irm https://slavkobojanic.github.io/kobra/install.ps1 | iex` on Windows), no sign-in needed, or
  `k toolchain install`; every download is checked against its SHA-256. The book is at
  https://slavkobojanic.github.io/kobra/, and `k docs` opens the copy installed with the toolchain.
  Kobra is MIT OR Apache-2.0.

### Language

- Components with a `view` of built-in elements, fine-grained reactivity without a virtual DOM, typed styles and themes compiled to CSS, element references, and `@Rendering(.client)` for components that render only in the browser. See [Components](../apps/components.md), [Views and built-in elements](../apps/views.md), [Reactivity](../apps/reactivity.md), [Styles and themes](../apps/styles.md).
- Kobra, a statically typed language that compiles to JavaScript: structs, enums with payloads, classes and interfaces, generics with explicit type arguments and read-only covariance, units, typed errors with `throws`, `catch` by type and `catch unknown`, `async` functions and `Stream`s, and modules with visibility checked at compile time. See [The basics](../language/basics.md), [Types](../language/types.md), [Structs, enums, classes and interfaces](../language/declarations.md), [Errors](../language/errors.md), [Modules and visibility](../language/modules.md).
- A package ships a font family with its theme: `Font(family:size:faces:)` lists `FontFace("fonts/Inter.woff2", weight: 700)` files of the package (workspace libraries included), and every app that uses the theme emits their `@font-face` rules with `font-display: swap`, preloads the body font, and serves the files under content-hashed names. See [Styles and themes](../apps/styles.md).

### Standard library

- Accounts and sessions with passwords and passkeys, `k.crypto` (keys, signatures, sealed values, JWTs) on the server and in the browser, and typed configuration with secrets read and validated at startup. See [Authentication and sessions](../apps/auth.md), [Bytes and cryptography](../apps/crypto.md), [Configuration and secrets](../apps/config.md).
- Entities and repositories over SQLite and Postgres with migrations generated from `schema.lock`, background and scheduled jobs, email, a typed HTTP client, and the browser's local storage and indexed collections. See [Persistence](../apps/persistence.md), [Background jobs](../apps/jobs.md), [Email](../apps/email.md), [Calling HTTP APIs](../apps/http.md), [Local storage](../apps/local-storage.md).
- Pages and routing with typed parameters and guards enforced on the server and in the browser, async data loading owned by its page, and forms with validation checked at compile time. See [Pages and routing](../apps/routing.md), [Async data loading](../apps/async-data.md), [Forms and validation](../apps/forms.md).
- The web platform as typed Kobra APIs: clipboard and sharing, location, notifications and Web Push, camera, microphone, recording and video calls, offline and installable apps, drag and drop, files, keyboard shortcuts, tabs and the screen, plus charts, money and time. See [Web platform APIs](../reference/web-platform.md), [Camera, microphone, recording and calls](../apps/media.md), [Offline and installable apps](../apps/offline.md), [Notifications](../apps/notifications.md), [Drag and drop](../apps/drag-and-drop.md), [Charts](../apps/charts.md), [Money](../apps/money.md), [Time and timers](../apps/time.md).

### Runtime

- `k build` makes one self-contained server executable, `dist/app`, with a minified, tree-shaken client and source maps that map back to your `.k` lines; it renders pages on the server and hydrates them, and runs under Docker, Compose and Kubernetes without other files. See [Deployment](../apps/deployment.md), [Debugging](../getting-started/debugging.md).
- Services with dependency injection checked per environment, protocols whose rpcs are called like functions with their shapes recorded in `rpc.lock`, server streams, WebSocket channels with presence across replicas, and workspaces whose apps call each other's services with service tokens. See [Services and dependency injection](../apps/services.md), [Protocols and RPC](../apps/protocols.md), [Server streaming](../apps/streaming.md), [Channels](../apps/channels.md), [Workspaces and services](../apps/workspaces.md).

### Tooling

- `editors/claude` is a Claude Code plugin with three skills (`kobra-language`, `kobra-apps` and `kobra-tooling`) that teach Claude to write idiomatic Kobra and to check it with the compiler. The plugin also starts the `k mcp` server, and installs with `/plugin marketplace add slavkobojanic/kobra` followed by `/plugin install kobra@kobra`. See [Editor setup](../getting-started/editor-setup.md).
- The VS Code extension shows the toolchain the language server runs (`K 0.1.0`) in its language status item, as the server reports it in `initialize`. See [Editor setup](../getting-started/editor-setup.md).
- A Kobra plugin for JetBrains IDEs (IntelliJ IDEA, WebStorm, PyCharm and the others, 2026.2 or later, free versions included): `.k` highlighting from the VS Code extension's grammar and `k lsp` through the IDE's built-in language server support. Each release attaches it as `kobra-intellij-<version>.zip`, installed with Settings → Plugins → Install Plugin from Disk. See [Editor setup](../getting-started/editor-setup.md).
- The JetBrains plugin shows a file's previews in a **Kobra Preview** tool window beside the editor: it runs `k preview` for the project, follows the selected file and the cursor, updates as you type, opens on a preview from its **▶ Preview** hint, and has Grid and Open in Browser in its toolbar. A setting keeps it from opening by itself. See [Preview views](../apps/previews.md), [Editor setup](../getting-started/editor-setup.md).
- `k fmt` formats Kobra code into its one canonical layout: the project or workspace, the paths named, or standard input (`-`); `--check` prints a diff and fails on unformatted files, and `--format json` reports each file. The language server formats documents and selections with it, and the VS Code extension formats on save. See [Everyday commands](../getting-started/commands.md), [The k command](../reference/cli.md), [Editor setup](../getting-started/editor-setup.md).
- The language is called Kobra (the command stays `k`, and source files `.k`), and it is dual-licensed MIT OR Apache-2.0, with both licences in every release archive. Until a stable release exists, the install scripts install the newest release candidate and say so. The VS Code extension is `slavkobojanic.kobra`, and the toolchain image `ghcr.io/slavkobojanic/kobra`. See [Installation](../getting-started/installation.md), [Quick start](../learn/quick-start.md), [Editor setup](../getting-started/editor-setup.md).
- `k mcp` is an MCP server for AI assistants (`claude mcp add kobra -- k mcp`): it serves the book, the design and compiler docs and the example apps built into `k`, plus the project it runs in, with tools to search and read them, explain a diagnostic code, and check Kobra code with the compiler. See [AI assistants (MCP)](../getting-started/ai-assistants.md), [The k command](../reference/cli.md).
- Kobra installs with one line and no sign-in: `curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh` (on Windows, `irm https://slavkobojanic.github.io/kobra/install.ps1 | iex`), from the public GitHub releases, checked against their `SHA256SUMS`. `k toolchain install` and `k self update` need no token either (one in `GH_TOKEN`, `GITHUB_TOKEN` or `gh auth token` is sent to GitHub's API only, to lift its rate limit), and each toolchain comes with its book, which the new `k docs` opens offline; the book is also online at https://slavkobojanic.github.io/kobra/. See [Installation](../getting-started/installation.md), [Quick start](../learn/quick-start.md), [The k command](../reference/cli.md).
- One `k` binary: `k new`, `k check`, `k fix`, `k test` (a browser and a server per test), `k dev`, `k build`, `k routes`, `k migrate` and `k lsp`, the language server behind the VS Code extension. See [Everyday commands](../getting-started/commands.md), [The k command](../reference/cli.md), [Editor setup](../getting-started/editor-setup.md), [Testing](../apps/testing.md).
- `k toolchain install`, `list`, `remove` and `default` manage installed toolchains, and `k self update` installs the newest release and makes it the default; downloads are checked against the index's size and SHA-256, and an interrupted install leaves nothing behind. `install.sh` and `install.ps1` install Kobra into `~/.k` in one line. See [The k command](../reference/cli.md), [Installation](../getting-started/installation.md), [Upgrading Kobra](../apps/upgrading.md).
- A project pins its toolchain: `k = "0.1"` in `k.toml` and the exact version in `k.lock`, with every command passed to the matching installed toolchain, `K_TOOLCHAIN` for one command, a format version in every lockfile, `@Deprecated` with machine-applicable fixes, and `k upgrade` to move a project to a newer toolchain. See [The k command](../reference/cli.md), [Project files and lockfiles](../reference/project-files.md), [Upgrading Kobra](../apps/upgrading.md).
- The VS Code extension shows previews in a **Kobra Preview** panel beside the editor: it runs `k preview --editor` for each project that enables previews, sends it the unsaved buffers, opens the panel when a `.k` file with previews becomes the active editor (setting `kobra.preview.autoOpen`), follows the active file and the cursor, and offers the **▶ Preview** CodeLens, an editor-title button, **Kobra: Open Preview** and a grid toggle. A preview error shows in the status bar until the next good build; clicking it goes to the error. See [Preview views](../apps/previews.md), [Editor setup](../getting-started/editor-setup.md).

### Preview features

- `k preview [path]` serves a project's previews in the browser: an index by file, and a panel per preview with a switcher, controls for its parameters (toggles, enum dropdowns, number steppers, text fields, and parsed fields for `Money`, `Duration` and `parse` types), an environment bar (colours, reduced motion, viewport presets, locale, offline, hold loading) and a grid of every preview of a file. Each preview mounts in a real browser frame with fresh hermetic containers, and every save remounts it, keeping the control values; a build with errors keeps the last good one under a badge. `--editor` speaks a JSON-lines protocol for the editor panels, and `k lsp` adds a **▶ Preview** CodeLens on each `preview`. See [Preview views](../apps/previews.md), [The k command](../reference/cli.md).
- `preview` declarations (preview feature `view-previews`): a view rendered in a chosen state, written in the file of the view, with typed parameters as its state and a body checked as test code (fixtures, fakes, `use` rules, `@Inject`). `k check` mounts each preview once with its defaults in the simulated tab the tests use and reports a failure as `K7520`; previews never reach a build. New diagnostics `K7504`–`K7509` check the declarations. See [Preview views](../apps/previews.md), [Testing](../apps/testing.md), [Diagnostics reference](../reference/diagnostics.md), [Design status](../reference/design-status.md).
- `@Worker` services run off the main thread in Web Workers, behind the `workers` preview: list it in `[package] preview` to use them. See [Work off the main thread](../apps/workers.md).

### Fixes

- Postgres text and enum columns are created with `COLLATE "C"`, so `sortedBy` and comparisons order text by code point on every database, as SQLite and Kobra's own `sortedBy` do, whatever the database's locale.
- The test fakes now write a `Double` as a JSON number. `TestHttpClient.serveJson`, `MemoryStorage`, the in-memory collections and `RecordingJobQueue` used to encode it as an object, so decoding the value back failed with `expected a number`.

### Documentation

- The Kobra book: a quick start, the Shelf tutorial in ten steps, guides for every part of the language and the standard library, nine example apps, and references generated from the compiler, every sample compiled against it.

<!-- /gen -->
