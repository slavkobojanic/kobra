# Documentation and the Docs Site

## Decision

**The book is reorganised into two front doors, as react.dev is: _Learn_, which teaches, and _Reference_, which answers.** Learn starts with a ten-minute quick start and a longer tutorial that builds one full-stack app across several pages, then explains how to think in K, then holds the existing language and app guides. Reference holds every keyword, annotation, element, style property, stdlib module, type and member, diagnostic, `k` command and flag, and project file, and most of it is generated. Examples sit between the two. The design topics and compiler docs stay out of the newcomer path, in a Contributing section.

- **The site stays on mdBook, with a custom theme.** A K-built docs site is the long-term goal and has a staged migration path (below), but it waits until K can build it without special cases.
- **Search is mdBook's built-in search now, then Pagefind,** a single binary run after `mdbook build`, plus a generated symbol index for exact API lookups. Nothing needs an account or a hosted search service.
- **The docs can't silently drift from the toolchain.** Every claim that can be checked by a test is: every `k` sample compiles (built), samples marked as runnable also run, the tutorial's projects are compiled and tested at every step, reference pages are generated from the compiler's own registries and must match what's committed, links and anchors resolve, every public item, diagnostic, annotation and flag is documented, every command line in the book parses, and `Planned` notes are tied to features that the test knows are unbuilt. A user-visible change's changelog fragment names the book pages it changes, or says why none.
- **Docs are versioned with the toolchain,** as [48-releases-and-versioning.md](48-releases-and-versioning.md#docs-versioning) decides: one directory per minor (`/0.6/`), `/` and `/latest/` for the newest, `/next/` for main; before 1.0 only the newest minor and `/next/` stay live.
- **Before the reorganisation, every existing page is audited** against the current compiler, prose included, and each finding that a test could have caught becomes that test ([the page audit](#the-page-audit)).
- **An interactive playground (the compiler built to WebAssembly) is a later item,** with its cost estimated below.

**Needs the project owner's sign-off** (nothing here is published or requires an account until then). The decisions are listed, with options and a recommendation each, once, in [48-releases-and-versioning.md](48-releases-and-versioning.md#owner-decisions); the ones this topic depends on are:

1. The docs site's host and domain, and whether pull requests get preview deployments ([Hosting](#hosting)).
2. Whether to adopt Pagefind, a third-party binary used at docs-build time only ([Search](#search)).
3. The CI system, and whether the docs checks are required on pull requests ([Docs ownership](#docs-ownership-in-review)).
4. Whether the design topics and compiler docs are published as a second book.

## Why now

The language has changed quickly, and the book has only been kept honest where a test checks it. The checks that exist (every `k` block compiles; the stdlib and diagnostics pages are regenerated) caught code drift but not prose drift. For example, at the time of writing:

- `getting-started/first-app.md` says tests aren't part of `k check` yet ("Neither is part of it yet … tests run with `k test`") and tells readers to "run `k test` next to it in CI", while `k check` runs the tests by default and has `--no-tests` (`crates/k_cli/src/cli.rs`, and `getting-started/commands.md` already says so). `apps/testing.md` repeats the advice, and `reference/design-status.md` still lists "the test run as part of `k check`" as planned.
- `reference/annotations.md`, `reference/keywords.md`, `reference/cli.md` and the element and style-property tables in `apps/views.md` and `apps/styles.md` are hand-written copies of tables the compiler holds as data (`crates/k_sema/src/stdlib/annotations.rs`, `elements.rs`, `css.rs`, the clap definitions).

Every internal link and anchor resolves today (checked with a throwaway script while writing this topic), so a link checker is cheap to add and starts green.

## Information architecture

Five parts, in this order in the sidebar, with the theme's header showing **Learn · Reference · Examples · Contributing** as tabs that jump to each part's first page:

| Part | Reader | Promise |
|---|---|---|
| Learn | someone new to K | read in order; nothing is used before it's explained |
| Guides (the current "The language" and "Building apps") | someone building with K | one topic per page, read in any order |
| Examples | someone who learns from whole programs | every example is a project in `examples/`, compiled and tested |
| Reference | anyone looking something up | complete, terse, searchable, mostly generated |
| Contributing | people working on K itself | how the docs, design and compiler fit together |

Learn and Guides are both teaching material, but they make different promises: Learn is a path, Guides are a library. The language and app chapters stay as they are (they are good reference-grade guides) and move behind Learn instead of being the first thing a newcomer meets.

### The quick start

**One page, about ten minutes, a client-only app: a tip splitter.** The reader enters a bill, picks a tip percentage and the number of people, and sees what each person pays. It is small enough to read in one sitting and touches, once each, the things every K app has:

1. `k new tip` and `k dev`, then the start page in the browser.
2. `@State` for the bill, tip and people; `@Derived` for the share. Typing updates the result without any wiring.
3. A `numberField` bound with `$`, and three `button`s for the tip with `.on(click:)`.
4. Extracting those buttons into a `TipOption` component with a `@Prop` and an `@Emits` (`k.ui` has no segmented control, so the reader builds one; that is the lesson).
5. A `style` block with a theme token, and seeing a compile error for a misspelt class.
6. One test with `render` and `screen.button(named:)`, run by `k check`.
7. `k build` and running `dist/app` (or the static output: the app has no server code, so it is static-only, [38-deployment.md](38-deployment.md#static-only-apps)).

It uses `Int` percentages and `Money` only if the money literal stays as simple as `24.50.usd` by then; otherwise `Double` with a formatter. The page ends with "You've built …", a recap list, and two links: the tutorial, and Thinking in K.

**As built (round 1(b)):** the project is `examples/learn/quick-start/` (named after its page rather than `tip/`, so every Learn page's project is found by its page's name), checked and built by `crates/k_cli/tests/examples.rs` like every example. The bill is a `textField` read with `Double.parse`, not a `numberField`, because `numberField` binds only an `Int`; the result is `Money`, built from cents and split with `allocated(into:)`. The app stays server-rendered, as `k new` makes it, so step 7 runs `dist/app`. Installation and editor setup are listed in the Learn part but keep their `getting-started/` paths until the redirects exist; `first-app.md` stays where it is, now using `k new`.

**The quick start needs `k new`** ([24-projects.md](24-projects.md#creating-a-project-k-new)), which isn't built. Writing five files by hand is what `first-app.md` does today, and it is the wrong first impression. Building `k new` is the first item of round 1(b) of the [plan](#implementation-plan); the quick start ships with it.

### The tutorial

**A longer walkthrough that builds _Shelf_, a reading list with accounts,** across ten short pages. Each page adds one capability and ends with a working app. Shelf is chosen because it needs every layer (a list, a form, a table, a protocol, pages with parameters, accounts, per-user data, tests, a build) without needing a third-party account, and because it is different enough from the examples (Notes is the closest; it stays as an example of a finished app rather than a lesson).

| Step | Page | Adds | Concepts introduced |
|---|---|---|---|
| 1 | Create the project | `k new shelf`, the layout, `k dev` | project, page, `view`, `k check` |
| 2 | Show a list of books | a `Book` struct, a hard-coded list, a `BookRow` component | `struct`, `List`, `for` in views, `@Prop` |
| 3 | Add books with a form | the add-book form, validation messages | `@State`, `Validatable`, constraint annotations, `@Emits` |
| 4 | Make it look right | a theme, classes, a dark variant | `theme`, tokens, scoped styles, variants |
| 5 | Save books on the server | `entity BookRecord`, a repository, `BookProtocol`, a service | `entity`, `repository`, `protocol`, `rpc`, `service … implements`, migrations, `do`/`catch` |
| 6 | Load books from the server | the list loads, shows pending and failure states, reloads after adding | `@Load`, `match` on load state, typed errors |
| 7 | A page per book | `/books/:id` with notes and a rating | page parameters, links, `routes.lock` |
| 8 | Accounts | sign up, sign in, each user's own shelf | `k.auth`, sessions, `requires`, guards |
| 9 | Test it | component, rpc and integration tests | `k.test`, fakes, `TestRemote`-style bindings |
| 10 | Ship it | `k build`, configuration, the database, running `dist/app` | `config`, secrets, `schema.lock`, deployment |

**Every step is a checked-in project:** `examples/learn/shelf/01-create/` through `10-ship/`, each a complete K project (`k.toml`, `App.k`, `src/`, lockfiles). The tip splitter is `examples/learn/tip/`. They live under `examples/`, as [48-releases-and-versioning.md](48-releases-and-versioning.md#docs-freshness-checks) requires, in their own `learn/` directory so the ten Shelf steps don't crowd the example list, and they are tested like every example (see [drift-proofing](#tutorial-projects)), and the page for step _N_ shows the files that changed since step _N−1_ as excerpts, so the reader can follow along and a reviewer sees code in the markdown diff.

**As built (round 1(b), steps 1–3):** step 1's project is diffed against a fresh `k new shelf`, and every later step against the one before it, by `crates/k_cli/tests/book_projects.rs`; lockfiles are left out of that diff, since `k build` writes them and `examples.rs` already checks the committed ones. A page shows a changed file as an excerpt (`// examples/learn/shelf/NN-name/…` as its first line, `// ...` between runs), and every added or changed line must be in one. Step 1 replaces the counter with a heading and a sentence, and so also replaces the generated test (one sentence on the page, pointing to step 3), because `examples.rs` requires every Learn project to run a test. `Book` has an `Int` `id` from step 2, used as the `for` key, since titles can repeat. Step 3 uses `@Form` (not in the table above, but a `Validatable` struct is only used through it), and clears the form after a book is added with `Form.reset()`, which `k.validation` gained for it ([15-forms-and-validation.md](15-forms-and-validation.md#the-forms-initial-value-and-its-value)). The tutorial index lists all ten steps, linking only the written ones.

**As built (round 1(b), steps 4–6):**

- **Step 4** declares the tokens it uses (colors for text, background, surface, border, primary and its text, three spaces, one radius) and a `variant dark matches .prefersDark`, with no switch: `k.ui.Theme` needs `@Inject`, which step 5 introduces, and the styles guide covers it. Styles have no test of their own, so its "Check it" only reruns the step 3 tests.
- **Step 5's entity is `BookRecord`, not `Book`.** `Book` is the `Codable` struct the browser shows, and an entity can't cross the wire; renaming the struct would have changed every file of steps 2–4 on page 5. The `Record` suffix is the one `examples/shop/paybox` uses. `Book.id` becomes a `Uuid` (`Uuid.random()` on the server), since K has no generated integer keys, so the three hard-coded books go and the list starts empty. The migration is the one `k migrate new "create book records"` writes, unedited.
- **Step 5 has two services,** `BookApi implements BookProtocol` on the server and `BookService` in the browser, because only a service may inject a protocol (K4005). The form, not the page, calls `BookService.add`, so it can report the saved `Book` (with the server's id) through `added`. The call's failure is caught there with `catch .rpcFailure`, so `do`/`catch` and an error enum's cases move from step 6 to step 5 in the index's table; step 6 keeps `match` on the error in `failed`. The repository's `all()` arrives in step 5, for the test that checks the database, and is served by step 6's `list` rpc.
- **Step 6 refreshes with `reload()`** rather than appending the reported book, so the list always shows what the server holds. Its `loading` block is only seen in tests until step 7 adds client-side navigation, since the first page is server-rendered with the books loaded; the page says so. A test of `failed` needs a fake protocol, which is step 9's subject.

**As built (round 1(b), steps 7–10):**

- **Step 7's page is `BookDetails at "/books/{id}"`,** in `books`: a page can't share the `Book` struct's name in its module, and the named URL keeps the `/books/…` the table above plans (written `{id}`, K's syntax, in the index). Notes and a rating are two `BookRecord` columns from a second, generated migration; `Book` gets them with defaults, because `rpc.lock` refuses a new required field (K8001), which the page explains in one sentence. The rating is an `Int` from 0 (not rated) to 5, picked with a `dropdown`; `get` and `review` throw a new `LookupError` (`notFound`), and `failed` sets `status(.notFound)`. `BookApi` gains a private `book(record:)`. The page's Try-its are a link with a wrongly typed parameter and removing `at` (K8007 from `routes.lock`).
- **Step 7 found a `k dev` bug:** a rebuild with an entity that has no migration was served (a new table) or failed with a code-generation message (a new column), while `k check` reported K8004. `k dev` and `k run` now count `schema.lock`'s errors (`k_codegen::schema_diagnostics`, the schema half of `project_diagnostics`) as build errors, before generating: `error: the project has N error(s)` and the last good build keeps serving (`crates/k_cli/tests/cli.rs`, `dev_reports_an_entity_without_a_migration`). `routes.lock` problems are left out, since a detected rename is recorded by the next `k build`; `k build` itself is unchanged.
- **Step 8 keeps accounts in `src/accounts/`,** after `examples/notes`, cut down: one `Credentials` struct (an `EmailAddress` and an 8-character password) for both forms, `SignUpError` and `SignInError`, no rate limits and no availability `@Check`. `BookRecord.ownerId` is `AccountId?`, because the table has rows (K8004 asks for a default or an optional), so the books of earlier steps belong to nobody; the page says so. Tests sign in with `SessionManager` and a fixed `AccountId` declared in each group, at the top of every test.
- **Step 9 changes only tests.** The `failed`-block test that step 6 lacked fakes `BookService` (`src/testing/OfflineBookService.k`, a test-only module), not `BookProtocol`: a protocol handler may not throw `rpcFailure` (K5506), and a test's in-process calls never fail with `.offline`. The step also adds rpc tests through `BookProtocol` (guards, another account's book, server-side validation with `expect throws`), the sign-up and sign-in forms with real accounts, and `TestClient` tests of the redirect, the title and the 404.
- **Step 10's setting is `ShelfConfig.signUpsOpen` (`SHELF_SIGN_UPS`, default `true`),** a new `SignUpError.closed` case, and an `.env.example`; secrets are explained through `DATABASE_PASSWORD` in `dist/app config` rather than one of Shelf's own, since a dev-generated token isn't printed and the tutorial must run without setup. It runs `dist/app` on a SQLite file (Postgres needs a server the reader may not have). Containers aren't shown: the toolchain image is Planned, and Learn pages use no Planned features.
- **K7503 now follows protocols** (it was deferred while steps 7–10 were written). A test group that reached an app config only through a protocol's server implementation (`SignUp.test.k` → `AuthService` → `AuthProtocol` → `AuthApi` → `ShelfConfig`) wasn't reported, and the run failed as a whole with `rpc failed: internal`. The walk in `k_sema::app::test_group` and `di::reaches_config` now steps from a protocol to its server implementations, unless the group binds the protocol itself (`tests/corpus/sema/app/tests/unbound-config-through-protocol`). Step 10 binds the config in both groups.

Tests are written from step 3 onwards in a short "Check it" section of each page, rather than only in step 9: step 9 then adds the kinds of test a newcomer hasn't met (rpc fakes, integration through the server).

### Thinking in K

Short concept pages, in the spirit of react.dev's "Thinking in React", placed after the tutorial and before the guides. Each explains one idea, with a small example, and links to the guide that covers it fully:

- **Why K** (the current `philosophy.md`, trimmed).
- **A tour of K** (the current `tour.md`, the guestbook): for experienced developers who want the whole picture on one page. It stays, retitled "K in one page".
- **Thinking in components:** split a screen into components, data flows down as props, changes flow up as events, state lives in the lowest component that needs it.
- **State, derived values and effects:** what re-runs and why; why most "effects" should be `@Derived`.
- **One language, two sides:** client and server code in one project, the boundary the compiler enforces, and why a `protocol` is the only way across.
- **Errors you can't ignore:** typed `throws`, `try`, and matching failures.
- **Let the compiler review you:** diagnostics, quick fixes, lockfiles and `@Breaking`, `k check` as the gate.
- **Coming from React, Vue, Svelte or Angular:** a mapping table (`useState` → `@State`, `computed` → `@Derived`, `v-for` → `for`, `@Input` → `@Prop`, services and DI, `fetch` → `rpc`), and the three habits to unlearn.

### The proposed `SUMMARY.md`

New files are marked _(new)_; generated files _(gen)_. Every existing page appears exactly once, in its new place.

```text
[K](index.md)                                            (new: landing page; introduction.md's opening, a code sample, the two front doors)

# Learn
- [Quick start](learn/quick-start.md)                    (new; replaces getting-started/first-app.md)
- [Installation](learn/installation.md)                  (move: getting-started/installation.md)
- [Editor setup](learn/editor-setup.md)                  (move: getting-started/editor-setup.md)
- [Tutorial: build Shelf](learn/tutorial/index.md)       (new)
    - [1. Create the project](learn/tutorial/01-create.md)
    - [2. Show a list of books](learn/tutorial/02-list.md)
    - [3. Add books with a form](learn/tutorial/03-form.md)
    - [4. Make it look right](learn/tutorial/04-style.md)
    - [5. Save books on the server](learn/tutorial/05-server.md)
    - [6. Load books from the server](learn/tutorial/06-load.md)
    - [7. A page per book](learn/tutorial/07-routing.md)
    - [8. Accounts](learn/tutorial/08-accounts.md)
    - [9. Test it](learn/tutorial/09-testing.md)
    - [10. Ship it](learn/tutorial/10-ship.md)
- [Thinking in K](learn/thinking/index.md)               (new)
    - [Why K](learn/thinking/why-k.md)                   (move: philosophy.md, trimmed)
    - [K in one page](learn/thinking/tour.md)            (move: tour.md)
    - [Thinking in components](learn/thinking/components.md)       (new)
    - [State, derived values and effects](learn/thinking/state.md)   (new)
    - [One language, two sides](learn/thinking/client-and-server.md) (new)
    - [Errors you can't ignore](learn/thinking/errors.md)           (new)
    - [Let the compiler review you](learn/thinking/compiler.md)     (new)
    - [Coming from React, Vue, Svelte or Angular](learn/thinking/coming-from.md) (new)

# Guides: the language
- [The basics](language/basics.md)
    - [Constants and variables](language/constants-and-variables.md)
    - [Functions](language/functions.md)
    - [Types](language/types.md)
    - [Strings and interpolation](language/strings.md)
    - [Units and number extensions](language/units.md)
    - [Collections](language/collections.md)
    - [Lambdas](language/lambdas.md)
- [Control flow](language/control-flow.md)
- [Structs, enums, classes and interfaces](language/declarations.md)
- [Errors](language/errors.md)
- [Modules and visibility](language/modules.md)
- [Documentation comments](language/doc-comments.md)
- [JavaScript interop](language/js-interop.md)

# Guides: projects
- [Project layout](apps/project-layout.md)               (move: getting-started/project-layout.md)
- [Everyday commands](apps/commands.md)                  (move: getting-started/commands.md)
- [The app declaration](apps/app.md)
- [Packages](apps/packages.md)
- [Workspaces and services](apps/workspaces.md)

# Guides: the user interface
- [Components](apps/components.md)
- [Views and built-in elements](apps/views.md)           (its element tables move to Reference; the page keeps the explanation)
- [Styles and themes](apps/styles.md)                    (its property tables move to Reference)
- [Reactivity](apps/reactivity.md)
- [Pages and routing](apps/routing.md)
- [Async data loading](apps/async-data.md)
- [Forms and validation](apps/forms.md)
- [Document metadata](apps/metadata.md)
- [Local storage](apps/local-storage.md)
- [Charts](apps/charts.md)
- [Drag and drop](apps/drag-and-drop.md)
- [Importing and exporting files](apps/files.md)
- [Keyboard shortcuts](apps/keyboard-shortcuts.md)

# Guides: the server
- [Services and dependency injection](apps/services.md)
- [Protocols and RPC](apps/protocols.md)
- [Server streaming](apps/streaming.md)
- [Persistence](apps/persistence.md)
- [Authentication and sessions](apps/auth.md)
- [Configuration and secrets](apps/config.md)
- [Endpoint pages](apps/endpoints.md)
- [Background jobs](apps/jobs.md)
- [Email](apps/email.md)
- [Calling HTTP APIs](apps/http.md)

# Guides: values and utilities
- [Time and timers](apps/time.md)
- [Money](apps/money.md)
- [Bytes and cryptography](apps/crypto.md)

# Guides: testing and shipping
- [Testing](apps/testing.md)
- [Deployment](apps/deployment.md)
- [Upgrading K](apps/upgrading.md)                       (new: the procedure, then one generated "Upgrading to 0.N" section per minor, cumulative, 48-releases-and-versioning.md)

# Examples
- [About the examples](examples/index.md)                (new: what each shows, how to run them)
- [Todos: components and local state](examples/todos.md)
- [Weather: loading data from an HTTP API](examples/weather.md)
- [Notes: a full-stack app with accounts](examples/notes.md)
- [Kanban: drag and drop in the browser](examples/kanban.md)
- [Stack: the smallest system of services](examples/stack.md)
- [Shop: a workspace of services with paybox](examples/shop.md)

# Reference
- [Using the reference](reference/index.md)              (new: what's here, search tips, what's generated)
- [Keywords](reference/keywords.md)                      (gen: from the lexer's keyword table)
- [Operators and precedence](reference/operators.md)     (gen: from the parser's precedence table)
- [Annotations](reference/annotations.md)                (gen: from k_sema/src/stdlib/annotations.rs)
- [Built-in elements](reference/elements.md)             (new, gen: from k_sema/src/stdlib/elements.rs)
- [Style properties](reference/style-properties.md)      (new, gen: from k_sema/src/stdlib/css.rs)
- [Standard library](reference/stdlib.md)
    - [`k.core`](reference/stdlib/k-core.md)             (gen, as today, for every module below)
    - [`k.auth`](reference/stdlib/k-auth.md)
    - [`k.charts`](reference/stdlib/k-charts.md)
    - [`k.crypto`](reference/stdlib/k-crypto.md)
    - [`k.db`](reference/stdlib/k-db.md)
    - [`k.device`](reference/stdlib/k-device.md)
    - [`k.drag`](reference/stdlib/k-drag.md)
    - [`k.encoding`](reference/stdlib/k-encoding.md)
    - [`k.http`](reference/stdlib/k-http.md)
    - [`k.observe`](reference/stdlib/k-observe.md)
    - [`k.rpc`](reference/stdlib/k-rpc.md)
    - [`k.server`](reference/stdlib/k-server.md)
    - [`k.storage`](reference/stdlib/k-storage.md)
    - [`k.test`](reference/stdlib/k-test.md)
    - [`k.text`](reference/stdlib/k-text.md)
    - [`k.time`](reference/stdlib/k-time.md)
    - [`k.ui`](reference/stdlib/k-ui.md)
    - [`k.validation`](reference/stdlib/k-validation.md)
- [Diagnostics](reference/diagnostics.md)                (gen: becomes an index; split below)
    - [K1xxx: syntax](reference/diagnostics/k1.md)       (gen: one page per code family, split from today's page)
    - [K2xxx … K8xxx](reference/diagnostics/k2.md)       (gen: one page per family that exists in codes.rs)
- [The k command](reference/cli.md)                      (gen: index from the clap definitions)
    - [k check](reference/cli/check.md)                  (gen: one page per subcommand, every flag)
    - [k build, k run, k dev, k test, k routes, k migrate, k lsp, k hash-password](reference/cli/build.md) (gen: one page per subcommand clap defines, so k new, k fix, k upgrade and k toolchain appear when built; `debug` is hidden)
- [Project files and lockfiles](reference/project-files.md) (partly gen: the k.toml key table)
- [Glossary](reference/glossary.md)                      (new)
- [Changelog](reference/changelog.md)                    (new: CHANGELOG.md, assembled from changelog fragments at release)
- [Design status](reference/design-status.md)            (partly gen: the Planned table from the book's Planned notes)

# Contributing
- [How the docs are built](contributing/docs.md)         (new: moves the authoring rules from docs/book/README.md into the book)
- [Writing Learn pages](contributing/learn-style.md)     (new: the style guide below)
- [Design topics and compiler docs](contributing/internals.md) (new: what docs/design and docs/compiler are, linked by repository path)

Dropped: introduction.md (folded into index.md), getting-started/first-app.md (replaced by the quick start
and tutorial steps 1 and 10; its "Planned" HMR note moves to apps/commands.md).
```

The two diagnostics and CLI lines that stand for several pages are written out in full by the generator; the generator also writes their `SUMMARY.md` entries between marker comments (`<!-- gen:diagnostics -->` … `<!-- /gen -->`), and the reference test fails when the committed entries differ, as it already does for stdlib modules.

**Moves keep old URLs working.** mdBook's `[output.html.redirect]` table maps every moved page (`getting-started/first-app.html` → `learn/quick-start.html`, `philosophy.html` → `learn/thinking/why-k.html`, …). A test checks that every page path that existed at the last release is either still a page or has a redirect, so a link someone saved never 404s.

**Design topics and compiler docs aren't published in the book.** They link with repository paths (`../compiler/grammar.md`) and are written for people changing K. The Contributing section explains them and links to them by repository path, as the book's README already asks. Once the repository has a public URL, a second, small mdBook ("K internals", with a `SUMMARY.md` generated from `docs/design/README.md`'s table and `docs/compiler/`) can be published next to the book; that is optional and outside this plan.

## Style guide for Learn pages

The book's tone stays as it is: second person, present tense, plain sentences, no hype, short paragraphs, and code that follows the design ([docs/book/README.md](../book/README.md)). Learn pages add these rules, and `contributing/learn-style.md` publishes them:

- **One idea per page, short.** A tutorial step is 800–1,500 words, a concept page under 1,000. If a page needs a second heading level below `##`, it is probably two pages.
- **Every page starts by saying what you'll do** ("In this step you'll save books on the server, so they survive a restart.") and **ends with a recap** (three to five bullets of what was learnt) and **next-step links** (the next page, and the guide that covers the topic fully).
- **Runnable code, always whole.** Every sample on a Learn page is from a checked-in project (an excerpt of `examples/learn/…`) or compiles on its own. Fragments that can't run are `k,ignore` only on concept pages, never in the quick start or tutorial.
- **"Try it" steps.** At least one per page: a small change the reader makes and what they should see, including making a mistake on purpose and reading the diagnostic. The expected output is shown, and it is checked (see [command transcripts](#command-transcripts)).
- **No forward references to advanced features.** A Learn page uses only what an earlier Learn page introduced. It may mention, in one sentence with a link, that more exists ("Effects can also … see Reactivity"). A test enforces the order for the tutorial: step _N_ may not use a keyword or annotation first introduced in a later step (it has a table of what each step introduces, the one above).
- **Plain language.** Define a term the first time it appears, in the sentence, not in a footnote. Prefer "the browser" and "the server" over "client" and "host". No "simply", "just", "obviously" or "easy". Spell out abbreviations once (RPC, SSR).
- **No Planned or Preview features in Learn.** If a Learn page needs a feature, the feature is built and stable before the page ships. Planned and Preview notes ([48-releases-and-versioning.md](48-releases-and-versioning.md)) belong in Guides.
- **Show the result.** Each tutorial step has a screenshot of the finished step (captured by the browser test from the step's project, so it can't go stale; see the plan), with alt text.

## Search

### Options

| | mdBook built-in (elasticlunr) | Pagefind | A K-built search (rpc or prebuilt index) |
|---|---|---|---|
| What runs | a JS index generated by mdBook, loaded whole on first search | a static, chunked index written by the `pagefind` binary after the build; a small JS UI | a K page that queries an rpc, or a prebuilt index shipped as a static file |
| Toolchain cost | none, it's on | one more binary at docs-build time (Rust; `cargo install pagefind` or a release binary; no npm) | needs the K docs site |
| Index size | whole index downloaded; the generated diagnostics and stdlib pages make it grow linearly | fetched in small chunks as you type; scales to large sites | an rpc needs a running server, so the site stops being static |
| Quality | word matching only; poor for `RemoteClient.at`-style names; no filtering by section | stemming, weighting, sub-results per heading, **filters** (Learn / Guides / Reference) and custom metadata | as good as we make it |
| Offline / static hosting | yes | yes | prebuilt index: yes; rpc: no |

### Decision

- **Now:** keep mdBook's search (it already works, configured in `book.toml`).
- **Next, with the owner's sign-off on a docs-build-time binary:** run Pagefind over `docs/book/book/` after `mdbook build`, replace the search box with Pagefind's UI in the custom theme, and tag pages with a `data-pagefind-filter="section:Reference"` attribute (the theme sets it from the page's part), so a search can be narrowed to Reference. Pagefind is only a docs-build dependency; the `k` binary, apps and users never need it.
- **Always: a generated symbol index.** The reference generator also writes `symbols.json`: every keyword, annotation, element, style property, stdlib module, type and member, diagnostic code and CLI command and flag, with its URL and a one-line summary (the doc comment's first sentence). A small script in the theme (`theme/k-symbols.js`) answers exact and prefix matches from it first ("`@Load`", "`K2406`", "`--no-tests`", "`Money.usd`") and puts full-text results under them. This is what makes API lookups good, independent of the full-text engine, and the same file is what a future `k doc` and the language server's "open docs" link use.
- **`k explain K2406`** (Planned) prints the diagnostic's page from the same generated source, so the terminal and the site never disagree.

## Site generator

### Options

1. **Stay on mdBook with a custom theme.** It is a Rust tool (`cargo install`), the book already builds warning-free on it, its hidden lines, includes, redirects and search are what we use, and the snippet test is built around its markdown. Its limits: one sidebar (no true top-level tabs), highlight.js 10 bundled (K highlighting is a separate script), and a theme that is Handlebars and CSS.
2. **A K-built docs site.** The docs as a K app: pages rendered on the server at build time (static-only output, [38-deployment.md](38-deployment.md#static-only-apps)), K components for the sidebar, tabs, callouts and the playground, search from a prebuilt index. It is the best possible dogfooding: if K can't build a good docs site, that's a bug in K. It needs things K doesn't have yet: reading a directory of markdown files at build time and rendering markdown to a view, prerendering every page of a static site, and highlighting from the compiler's lexer.
3. **Another static generator** (Docusaurus, VitePress, Astro Starlight). Rejected: they need npm, and the repository has none.

### Decision: mdBook now, a K-built site when K can build it

- **Phase 1 (now): mdBook with a custom theme.** Header tabs (Learn, Reference, Examples, Contributing) linking to each part, a landing page, callout styles for Planned, Open question, Try it and Recap, and a version switcher. K highlighting moves from `theme/k-highlight.js`'s copied keyword lists to the compiler's own lexer: an mdBook preprocessor (mdBook's preprocessor protocol is JSON on standard input and output) in the `xtask` crate, the same one [48-releases-and-versioning.md](48-releases-and-versioning.md#docs-freshness-checks) uses to substitute `{{k_version}}`, that tokenises every `k` block with `k_syntax` and emits highlighted HTML. Keywords can then never fall behind the grammar. The current script stays as the fallback until the preprocessor exists.
- **Phase 2: the content format stays the source of truth.** Every page stays markdown with the book's conventions (hidden `~` lines, `k,ignore`, project-file comments, excerpts), and every generated page is generated as markdown. Nothing ties the content to mdBook except `SUMMARY.md` and `book.toml`, so changing the renderer later is a renderer change, not a rewrite.
- **Phase 3: a K-built site, when these exist in K:** static-only builds that prerender many pages from data, build-time reading of project files (or a generator step that turns the markdown into K data), a markdown-to-view renderer in the stdlib or a package, and highlighting from the lexer. The docs app then lives in `docs/site/` as a K project, reads `docs/book/src`, and must render every page the mdBook build renders (a test compares the page lists and the text of each page). The switch happens only when the K site is at least as good on page weight, search and accessibility; until then mdBook is what's published.

## Drift-proofing

Each check is a `cargo test` (so it runs locally, needs no CI vendor, and fails with a message saying how to fix it), except the docs-ownership check, which needs a diff base.

### The page audit

The checks below catch drift from now on; they don't find what has already drifted in prose. So before the reorganisation, **every page in `SUMMARY.md` is read against the compiler at one commit** (taken after the concurrent language changes have landed), and the audit leaves a record and new checks behind rather than only fixed pages.

- **What is checked on each page, claim by claim:** commands, flags and their output (against `crates/k_cli/src/cli.rs`, and by running them on a scratch project); status words ("not yet", "currently", "Planned", "isn't built", "will"), each confirmed or turned into a `Planned` note with an id; behaviour the prose describes (what renders, what runs when, what fails and how), against the design topic and the test that pins it; every diagnostic code named, against `codes.rs`; every inline-code name that looks like a K symbol (`` `RemoteClient.at` ``, `` `@Load` ``), against the stdlib and annotation registries; hand-copied tables, against the registries; and example pages, against the projects in `examples/`.
- **Every finding is fixed in the page and classified:** *checkable* (a test or a `stale.txt` pattern would have caught it, and the audit adds it, so it can't recur), *pinnable* (the claim is true but no test pins it; the audit adds a `k,test` block or a runtime test, or records it as unpinned), or *judgement* (wording, order, missing explanation).
- **The ledger** `docs/book/check/audit.toml` records, per page, the commit it was audited at and its unpinned claims. A test fails when a `SUMMARY.md` page has no entry, so a new page is audited when it is added, and the release lists pages not re-audited since the last minor whose design topic changed ([48](48-releases-and-versioning.md#docs-freshness-checks)).
- **Known findings to start from:** the `k check`-runs-tests claim on `getting-started/first-app.md`, `apps/testing.md` and `reference/design-status.md` (above), and eight broken anchors in `docs/design` and `docs/compiler` (`44-channels.md`, `32-http-client.md`, `33-crypto.md` twice, `codegen.md`, `server.md` twice) that the link test will report.

The procedure and the batches it is split into are in [docs/compiler/documentation.md §5](../compiler/documentation.md#5-the-page-audit).

### Samples that run

Built: every ` ```k ` block compiles (`crates/k_sema/tests/book.rs`). Added:

- **`k,test` blocks** run the block's `test` declarations with the test runner, with the chapter's project files; the book test fails when one of them fails. Concept pages use it to show behaviour with an assertion instead of prose ("this prints 3" becomes `expect total == 3`).
- **`k,render` blocks**, followed by a ` ```html,output ` block: the component is rendered on the server (the `k.test` `render` path) and its HTML, normalised (whitespace, generated ids), must equal the output block. For the few pages that show what a view produces.

K has no `print`, so "expected output" is expressed as tests and rendered HTML rather than as standard output.

### Command transcripts

A ` ```console ` block on a page that names a project (`<!-- project: examples/learn/shelf/03-form -->`) is a transcript: lines starting with `$ ` are commands, the rest is their expected output. The test runs the real `k` binary in a temporary copy of the project, normalises paths, durations and ports, and compares. A Try-it step that applies a patch first ("change `.page` to `.pgae`") names it: `<!-- apply: patches/03-typo.diff -->`. This is what keeps diagnostics messages, `k check` summaries and `k new` output on Learn pages true.

Separately, **every `k …` command line in a `bash` or `console` block must parse**: the test feeds it to the clap definition (`Cli::try_parse_from`) without running it. A renamed or removed flag fails the book, not a reader. Lines for Planned commands carry a `# planned` comment and are checked to be unparseable instead, so the test notices when the command is built.

### Tutorial projects

`crates/k_cli/tests/examples.rs` already checks every example; it is extended to the projects under `examples/learn/`: `k check` (which runs the tests) must pass, `k build` must succeed, and the committed lockfiles must match. `crates/k_cli/tests/book_projects.rs` adds:

- **Consecutive steps differ only where the page shows it.** For each file that differs between step _N−1_ and step _N_, the step's page must contain an excerpt of that file covering the changed lines. Nothing changes off-page.
- **The finished tutorial is a real app.** The last step runs in the browser test (`crates/k_cli/tests/dist_browser.rs` style, headless Chrome when present), which also writes each step's screenshot; a screenshot that changes is reviewed like a golden file (`K_BLESS=1`).
- **The introduction order holds**, using the table on the tutorial index page.

### Generated reference pages

Built: the diagnostics and stdlib pages (`crates/k_corpus/tests/book_reference.rs`, `K_BLESS=1` to regenerate). Added, in the same test and with the same bless flow:

| Page | Source |
|---|---|
| `reference/annotations.md` | the annotation registry, `crates/k_sema/src/stdlib/annotations.rs` (each entry gains a doc string and a "where allowed") |
| `reference/elements.md` | the element registry, `crates/k_sema/src/stdlib/elements.rs` |
| `reference/style-properties.md` | the property table, `crates/k_sema/src/stdlib/css.rs` |
| `reference/keywords.md`, `operators.md` | the lexer's keyword table and the parser's precedence table |
| `reference/cli.md`, `reference/cli/*.md` | the clap definitions (`clap::CommandFactory` on `k_cli`'s `Cli`), including each flag's help text and default |
| the `k.toml` table in `reference/project-files.md` | the manifest's typed configuration, between `<!-- gen -->` markers; the prose around it stays hand-written |
| `reference/diagnostics/*.md` | today's generator, split per family |
| the Planned table in `reference/design-status.md` | the book's `> **Planned.**` notes (below) |
| `symbols.json` | all of the above |

Hand-written explanation stays possible on generated pages: the generator copies a `<!-- keep -->` … `<!-- /keep -->` region from the committed file into the regenerated one.

### The web platform page

**Which browser APIs K wraps is a generated reference page, `reference/web-platform.md`, from one data file, `docs/book/data/web-apis.toml`** (decided 2026-11-12, at the owner's request for a place that tracks what K supports of the JavaScript and browser platform). One row per API, named as its spec names it, grouped by area: a status (`supported`, `partial`, `planned` with its tier or feature id, `not-planned` with why), the K declarations that cover it, where they run, the real-browser tests that check it in Chromium and Firefox, the design heading that decides it, and a one-line note for gaps and engine differences. The page shows a summary of counts per area, then each area's table.

- **Data, not prose,** because three things can drift and each can be checked: the K API (it must resolve in the stdlib), the runtime (every file of `crates/k_runtime/js/runtime` is claimed by a row or listed as internal, so a new binding can't land without the page), and the engine claims (each names a `#[test]` of `chrome.rs` or `firefox.rs`). `crates/k_sema/tests/web_platform.rs` checks them and regenerates the tables with `K_BLESS=1`, as the design-status table is.
- **No new support claims.** Statuses come from [45's inventory](45-web-platform.md#inventory) and the topics it links. A Tier 3 entry is `planned` with tier 3; an entry 45 calls "covered elsewhere" is `supported` through the K API that covers it when that API is built (History through the router), and `not-planned` when it is covered by another mechanism (server-sent events by streams and channels).
- **WebKit gets no column** until the real-browser suite drives it ([45's open questions](45-web-platform.md#open-questions)); Safari behaviour in a note is from the engines' documentation. A link to the page sits in the stdlib overview, the page-and-device guide and the JavaScript interop page's "What isn't available".
- **Rejected:** a hand-written page with a test that only parses it (the table would be the data, in a harder format to check); an mdBook preprocessor (the book has none yet, and the bless flow already exists); generating the rows from the stdlib's `@Platform` declarations (most rows are about APIs K doesn't wrap, which no declaration names).

### Coverage

The same test fails when something user-visible has no documentation:

- every public stdlib item, and every member [39-doc-comments.md](39-doc-comments.md#what-must-be-documented) requires documenting, has a `///` comment;
- every diagnostic code in `codes.rs` has an example and a fix (built), and a message pointing to its page (`help: see …/reference/diagnostics/k2.html#k2406` once the site has a URL);
- every annotation, element and style property in the registries has a doc string;
- every CLI subcommand and flag has help text (clap's `Command::debug_assert` plus a check that `help` is set);
- every stdlib module is in `SUMMARY.md` (built), and every generated page is.

### Links and anchors

A Rust test over `docs/book/src` resolves every relative link to a page and every `#anchor` to a heading id, computed the way mdBook computes them (lowercase, punctuation dropped, spaces to `-`, `-1` suffixes for repeats). It also checks `docs/design` and `docs/compiler` (their links into each other and into the book), the `redirect` table's targets, and the URLs that diagnostics print. External links aren't checked in `cargo test` (no network in tests); an opt-in `K_CHECK_EXTERNAL_LINKS=1` run does it.

### Design topics and the book

Each `docs/design` topic gets a status line under its title, `> **Status:** built`, `partly built`, `designed` or `proposal`, and, when built or partly built, a `> **Book:** [Persistence](../book/src/apps/persistence.md)` line. The test checks that every built or partly built topic links to an existing book page, that every book page in Guides links back to at least one topic by repository path (they mostly do already), and that `docs/design/README.md` lists every topic file.

### Planned notes are tied to the code

Every `> **Planned.**` note carries an id, `> **Planned** (k-new).` (several: `(k-fmt, doc-blocks)`; inline in a sentence or table cell: `(**Planned** k-fmt)`), and `reference/design-status.md`'s table is generated from the notes. The ids are rows of the compiler's feature table (`k_sema::features`, [48-releases-and-versioning.md](48-releases-and-versioning.md#stability-levels)), which records each feature as `planned`, `preview` or `stable`; a Planned note whose row says `stable`, or a Preview note whose row doesn't say `preview`, fails the test. The table starts in round 1 with only `planned` rows; `preview` rows come with the stability annotations. Where the table can't be trusted alone, an id also names a probe: `k-new` is checked by "`k new` parses as a subcommand", `k-fmt` by "`k fmt` parses", `iso-4217` by "the money currency table has more than five entries". When a probe starts succeeding the test fails with "`k new` is built: remove the Planned notes with id `k-new` (getting-started/first-app.md:8, getting-started/project-layout.md:129)". Notes whose feature can't be probed are listed with the date they were written and reviewed at each release.

### Prose staleness heuristics

A list, `docs/book/check/stale.txt`, of patterns that must not appear in prose, `text` blocks or `bash` blocks, each with the reason and the replacement, for example:

```text
# pattern                                  # why
\b(font|background|max|min|line)-[a-z]+:   CSS properties are camelCase in K (K3214)
the compiler is (still )?being written      the compiler exists
Prebuilt releases aren't published yet     remove when release binaries exist (release topic)
tests run with `k test`                     k check runs the tests
```

`k` blocks don't need it, since they're compiled. When a language change removes syntax, its PR adds the old form to `stale.txt`, so the next mention anywhere fails. Diagnostic fixes already describe most removed syntax; the list can be seeded from `check/diagnostic-fixes.txt`'s rewrite fixes.

### Docs ownership in review

**A user-visible change must say what it does to the docs.** [48-releases-and-versioning.md](48-releases-and-versioning.md#changelog-fragments) requires a changelog fragment in every pull request that changes `crates/`, `editors/` or `docs/book/` (a `kind: internal` fragment is the opt-out) and owns the fragment format, including the `book:` field this topic relies on:

- **A fragment of a user-visible kind** (`breaking`, `deprecation`, `language`, `stdlib`, `runtime`, `tooling`, `preview`) **has a `book:` field**: the book pages the change affects, or `none` with a reason (`book: none: error message wording only`). It is optional on `fix`; a `docs` fragment is itself the docs change.
- **The docs gate checks the field against the diff.** Each listed page must be changed in the same diff (or be a generated page that the reference test regenerated). `book: none` is allowed but visible in review, which is the point: the reviewer decides whether "none" is true.
- **The gate is a small Rust program** in the `xtask` crate (`cargo xtask docs-gate <base-ref>`), reading `git diff --name-only <base>...HEAD` and the added fragments, so it runs locally before pushing and in whatever CI is chosen. The generated-page and coverage tests catch the mechanical part; this catches the prose part by forcing a decision in review.

There's no CI configuration in the repository; **which CI runs the gate, and whether it is a required check, needs the owner's sign-off** (the CI and required-checks rows of [48's owner decisions](48-releases-and-versioning.md#owner-decisions)). Until then a contributor runs it locally, and `cargo xtask release check` runs it against the previous release tag.

## Docs and releases

Release mechanics (tags, binaries, the changelog, `cargo xtask release`) are in [48-releases-and-versioning.md](48-releases-and-versioning.md), and so is the versioned layout of the site (`/0.N/`, `/` and `/latest/`, `/next/`, `/versions.json`, the version switcher's data). This topic relies on it and adds:

- **The site's information architecture is versioned with it:** a release's docs are one book from the release commit, so Learn, Guides and Reference always describe the same release. A page moved between releases has a redirect in both.
- **Features new in a minor are marked** with `> **New in 0.N.**` on the guide that introduces them; the stale-text test fails on a marker older than two minors, so they are removed.
- **"Upgrading K" (`apps/upgrading.md`) is one page:** the general procedure first (today: update `k = "…"`, run `k check`, apply the machine-applicable fixes, run `k migrate` if `schema.lock` changed, mark deliberate lockfile breaks with `@Breaking`; once built: `k upgrade`, which does all of that), then the "Upgrading to 0.N" sections the release generates from breaking and deprecation fragments, kept for every earlier minor so the single live minor's site covers a project several minors behind.
- **Before 1.0 the version switcher lists only the live minor and `next`;** the archived minors are linked from the changelog page, each to its release's attached site archive.
- **A release is blocked when any docs test fails** on its commit (48 lists the docs tests in its gate; the ones this topic adds join it).

## Hosting

Nothing is published until the owner chooses. The built site is static HTML (plus the Pagefind index), so any static host works:

| Option | Notes |
|---|---|
| GitHub Pages (or the code host's equivalent) | free with a public repository; deploy from CI; versioned folders are plain directories |
| Cloudflare Pages / Netlify | free tiers; preview builds per PR; needs an account |
| Object storage behind a CDN (S3 + CloudFront, R2, …) | most control; needs an account and some setup |
| Self-hosted, served by a K app | the dogfooding route once Phase 3 exists; needs a server to run |

**Decision needing sign-off** (listed with the other hosting decisions in [48-releases-and-versioning.md](48-releases-and-versioning.md#owner-decisions), recommended there: GitHub Pages if the source is on GitHub, PR previews as a CI artifact): the host, the domain, and whether PR previews are wanted. Until then the site is built locally (`cargo xtask docs build`, or `mdbook build docs/book` for one version) and in tests.

## Interactive playground (later)

**A page where you edit K and see it compile and run, with no install,** as react.dev's sandboxes do. The compiler front end is pure Rust with no system dependencies (`k_syntax` depends on `rowan`, `k_sema` on `rowan` and `toml`), so it builds for `wasm32-unknown-unknown`. The stdlib is already embedded as data.

| Stage | What works | Cost (one person) |
|---|---|---|
| A. Check | type-checking and diagnostics, with their fixes, as you type; hover and completion via the LSP's analysis | 1–2 weeks: a `k_wasm` crate exposing `AnalysisHost` through `wasm-bindgen`, an editor component (CodeMirror loaded from a CDN, or a plain textarea first), size work |
| B. Run client-only apps | codegen to JS in the browser; the output runs in a sandboxed `iframe` with the JS runtime | 2–3 weeks: `k_codegen` depends on `k_db` (rusqlite, tokio-postgres) and must feature-gate it out; bundling in memory; sandboxing |
| C. Full-stack | server code runs in a Web Worker on the embedded JS runtime, with the in-memory fakes `k.test` uses for the database, mail and clock | 4–6 weeks: the server runtime's host functions (today provided by the Rust server) reimplemented for the worker |

Downloads are several megabytes of wasm, loaded only on playground pages. Learn pages would then get "Open in playground" on every sample, and the quick start could be done entirely in the browser. It is static, so it needs no hosting beyond the docs site. It is not in this plan's critical path; stage A is a good first contribution once the reorganisation ships.

## Implementation plan

**This is the one plan for this topic and [48-releases-and-versioning.md](48-releases-and-versioning.md);** [docs/compiler/release.md §12](../compiler/release.md#12-implementation-plan) details the release steps and [docs/compiler/documentation.md](../compiler/documentation.md) the docs ones, and both are scheduled by these rounds. Every item starts with its failing test. Nothing in rounds 1–4 publishes anything or needs an account.

### Round 1: the docs are true, and a newcomer can start

**(a) Audit and drift-proofing.** The checks first, so the audit's fixes are locked in as they land. Items 1–6 are built and the audit is done (2026-11-09); what the checks and the audit caught is in [documentation.md §5](../compiler/documentation.md#5-the-page-audit):

1. The link-and-anchor test (`book_links.rs`) over the book, `docs/design` and `docs/compiler`: it fails on the eight known broken anchors; fix them.
2. The command-line-parses test (`book_commands.rs`) and `docs/book/check/stale.txt` with the entries above: they fail on the three `k check`-runs-tests pages; fix them.
3. `k_sema::features` with `planned` rows only, `Planned (id)` notes in the book test, the probes, and the generated Planned table of `design-status.md`.
4. The inline-symbol test: inline-code text in prose that parses as a qualified K name or an annotation must resolve in the stdlib or annotation registry (allowlist for deliberate non-symbols).
5. The ledger test (every `SUMMARY.md` page has an `audit.toml` entry), then **the audit itself**, in the batches of documentation.md §5, against one commit after the concurrent work lands. Each finding is fixed and, when checkable, becomes a `stale.txt` pattern or a test in the same change. Done against `0dd7625` and its working tree; `stale.txt` now also reads the example apps' READMEs, which make the same claims as the example chapters. The audit found the book claiming behaviour the compiler doesn't have yet, mostly warnings and quick fixes; each became a `Planned (id)` row of `k_sema::features` or was reworded, and the four places where the code and the design disagree are listed in documentation.md §5 for a decision. Claims of quick fixes that don't exist were the other repeated finding, so a test now requires a sentence that gives a diagnostic a quick fix to name its code, and a corpus case to pin that fix.
6. `crates/k_cli/tests/version.rs` (48's step 1), because Learn pages show `k = "…"` and install commands; `{{k_version}}` substitution in `book.rs`.

**(b) Learn: the quick start and the tutorial.**

1. `k new` ([24-projects.md](24-projects.md#creating-a-project-k-new)), test first: the generated app passes `k check` and `k build`; the `k-new` probe then fails until its Planned notes go.
2. `examples.rs` extended to `examples/learn/`, and `book_projects.rs` (empty passes) with the step-diff-on-page and introduction-order rules.
3. `console` transcripts (`book_transcripts.rs`) and `k,test` blocks, needed by the Try-it steps.
4. `examples/learn/tip/`, then `learn/quick-start.md`, with its transcripts.
5. Shelf, one step at a time, each step's project before its page: `examples/learn/shelf/01-create/` … `10-ship/` and `learn/tutorial/01-create.md` … `10-ship.md`, then the tutorial index with its introduction table. Screenshots come from the browser test (skipped without Chrome).
6. The Learn part at the top of `SUMMARY.md`, with installation and editor setup moved into it, `first-app.md` dropped, and the redirect test and redirects for those three pages. The rest of the reorganisation waits for round 2.

### Round 2: Reference and structure

1. Generated reference pages, one failing `book_reference.rs` test each, then the generator and `K_BLESS=1`: annotations, elements, style properties, keywords, operators, the CLI, the `k.toml` table; the diagnostics split per family with `SUMMARY.md` markers; coverage checks; `symbols.json`.
2. `k,render` blocks.
3. The full reorganisation: the redirect test over every current page, the moves and the new `SUMMARY.md`, Thinking in K, `examples/index.md`, the glossary, the Contributing pages.
4. Design status lines and their test.

### Round 3: release machinery, local

1. Changelog fragments and the `xtask` crate (release.md step 9), with the `book:` field and `cargo xtask docs-gate` tested on fixture diffs. From here every pull request adds a fragment. Built 2026-11-12 (`crates/xtask`, `changes/`; see release.md's as-built notes).
2. `k --version` with commit and date; the toolchain requirement, the `k.lock` pin, delegation and `K_TOOLCHAIN` (release.md steps 1–3). Steps 2 and 3 built 2026-11-12 (`K8107`, `K8201`; see release.md's as-built notes).
3. Lockfile `format` keys with `tests/compat/locks/` (step 5), and the 0.1 wire fixtures (step 10). Built 2026-11-12 (`K8202`; see release.md's as-built notes).
4. `k fix` (step 6); `@Deprecated`, `@Preview`, `@Internal`, the `preview` rows of the feature table, dependency warnings capped, and the badges in the stdlib reference (step 7).

### Round 4: upgrades, distribution and the site

1. `k upgrade` (step 8); `k toolchain` and `k self update` against the fake index (step 4); `install.sh` and `install.ps1` (step 11). `k upgrade` built 2026-11-12, with `docs/book/src/apps/upgrading.md`'s procedure and its generated part (item 4's "upgrade guide's procedure page and its generated sections").
2. The site build: `cargo xtask docs build` with the mdBook preprocessor (version substitution, then lexer highlighting), the `/0.N/`, `/next/`, `/` layout, `versions.json`, redirects (step 12); the theme (header tabs, callouts, landing page, version switcher, `k-symbols.js`).
3. `serverInfo.version` in the language server and the extension (step 14).
4. The CI workflow files and the test that every gate row appears in them (step 13); the upgrade guide's procedure page and its generated sections.

### Round 5: the first release

1. A local `0.1.0-rc.1` dry run (step 15), fixing the runbook from what it finds.
2. **After the owner's sign-off** ([48](48-releases-and-versioning.md#owner-decisions)): CI switched on with its required checks, hosting for binaries and docs, Pagefind, the registry, the marketplaces, then `0.1.0`.
3. Later: the playground, stage A.

## Why

- **A path and a library are different products.** Today a newcomer meets Installation, then "Your first app" asking them to write five files by hand, then a 378-line tour, then the full language. React's Learn section works because it promises an order and keeps it; the existing chapters are good as guides and keep that role.
- **Checked docs stay correct; unchecked docs drift.** The parts of the book that are tested (samples, generated pages) are right; the stale parts found while writing this were all prose or hand-copied tables. So the plan generates what the compiler knows as data and tests what readers will type or see.
- **mdBook is enough for now and costs nothing new**; a K-built site is better dogfooding, but building it before K can build it well would mean special cases that make K look worse, not better.
- **Search must be good at names**, which full-text engines aren't; a symbol index from the generators is cheap and makes the reference usable.

## Open questions

- Whether the tip splitter uses `Money` (depends on how simple money input and formatting are by then).
- Whether the "K internals" book (design topics and compiler docs) is published at all, or left to the code host's markdown rendering.
- How long to keep old minor versions' docs live after 1.0.
