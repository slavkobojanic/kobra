# Previews

## Decision

**A `preview` declaration renders a view in a chosen state, next to the code that defines it,** in the spirit of SwiftUI's `#Preview`. The VS Code and JetBrains plugins pick previews up automatically and show them in a panel beside the editor, with a switcher, typed controls, an environment bar and a grid of every state. `k preview` serves the same panel in a browser for every other editor. Decided 2026-11-24, in an interview with the owner.

```k
// Stepper.k
public component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1
    @Prop let max: Int? = nil
    @Prop let disabled: Bool = false
    ...
}

preview (value: Int = 1, step: Int = 1, disabled: Bool = false) {
    Stepper(value: bind value)
        .step(step)
        .disabled(disabled)
}

preview "at max" (value: Int = 99) {
    Stepper(value: bind value).max(99)
}
```

```k
// CartPage.k
public page CartPage { ... }

preview "signed-in cart" (items: Int = 3) {
    use FakeCartService for CartService
    @Inject let cart: FakeCartService
    @Inject let sessions: SessionManager

    await sessions.signIn(userId: .sample)
    cart.seed(count: items)

    CartPage()
}
```

**Previews are a preview feature** (`view-previews` in `k_sema::features`, [48](48-releases-and-versioning.md#stability-levels)): a project lists `preview = ["view-previews"]` in `[package]` to write one (K8108 otherwise), so the syntax can change before it is stable.

## Declaring a preview

- **`preview` is a top-level declaration written inline, in the file of the view it shows,** after or between the file's other declarations. It is never public, doesn't count toward the one-public-declaration rule ([04-components.md](04-components.md#file-shape)), and never reaches `k build`'s output: the compiler drops previews before code generation for `.production`, and for `.development` outside `k preview`.
- **A file may hold several previews.** The name is optional: `preview "sold out" { }` or `preview { }`. An unnamed preview is labelled after the type of the view it renders (`Stepper`). A file has at most one unnamed preview, and two previews in a file can't share a name (both errors). The rendered view's type is only the label: a preview may render any view, so a file of small helpers can preview them together.
- **The body is optional setup, then exactly one view.** Setup is what a test group allows: `use X for Y` rules, `@Inject let`, and statements, including `await` and `try`. A thrown error fails the preview ([below](#k-check)). The last element is a view expression: a component, a layout given placeholder content, or a page.
- **Parameters are typed and have defaults:** `preview "name" (label: Type = default, …)`. They are the preview's state, so a `@Binding` takes `bind value` and writes back, which moves the control. A parameter without a default is an error, since the panel must render before anyone touches a control. A literal can't be bound ([04-components.md](04-components.md#two-way-binding)), so a fixed-state preview of a `@Binding` component also uses a parameter, as `"at max"` does.

## What a preview can reach

**A preview's body is test-only code inside a production file.**

- **Inside a `preview`, names resolve as in a `.test.k` file of the same module:** fixtures from public extensions in `testing` modules (`.sample`, `.soldOut`) without an import, fakes (`service FakeX implements X`), and `testing` modules imported by the file. An import of a `testing` module is allowed in a production file only if every use of it is inside a `preview`. A use elsewhere is the existing test-only-import error, at the use. The import is dropped with the previews.
- **The environment is hermetic.** Every preview gets a fresh server container and browser container, as a test does ([21-testing.md](21-testing.md#which-side-a-test-runs-on)): the `k test` defaults for every stdlib interface that does I/O (`TestClock`, `MemoryStorage`, `TestHttpClient`, `RecordingEmailProvider`, …), an in-memory SQLite database with every migration applied, and one session. Precedence is the same: the preview's `use` rules override `App.k`'s `.test` rules, which override the defaults. App configs reached by the graph are bound by `use Config.sample for Config`, as in a test. A preview never reads `.env`, the development database or the network.
- **Protocol calls go through the real wire format** to the server container's implementations, as in tests. A fake bound by `use` lives where its target is used.

## What renders

- **Components and layouts render as written.** A layout's slot gets a neutral placeholder.
- **A page renders inside its layouts, as the router shows it after navigating to it** (unlike a test's `render`, which renders the page bare): its `@Load` fields start at once, `loading`, `view` or `failed` follows as they resolve, its `metadata` sets the panel's title, and guards apply, so a page that `requires Authenticated` shows the redirect unless the setup signs in.
- **A navigation from a preview stays inside it,** showing the new page in its layouts, as [21-testing.md](21-testing.md#a-navigation-shows-its-page) does for tests. *Reset* returns to the preview's own view.

## `k preview`

`k preview [path]` is the preview server: one process that compiles a file's previews, runs each one's hermetic server container in the runtime `k dev` uses, and serves the preview client with the panel's chrome around it.

- **Standalone,** it opens `http://localhost:<port>/` in the default browser, listing every preview in the project (or in `path`), with the same panel the editors embed.
- **From an editor,** the plugin starts it (`k preview --editor`), points a webview at it and sends it the open buffers itself ([docs/compiler/preview.md](../compiler/preview.md#5-the-editor-protocol)), so a preview updates as you type, debounced, whenever the file type-checks. While the file has errors, the last good render stays, under a badge naming the first error. Control values are kept across rebuilds by parameter name and type. A renamed or retyped parameter goes back to its default.
- **A rebuild remounts the preview** with fresh containers and the kept control values, so setup runs again and `@State` starts over. Hot replacement that keeps `@State` waits for `k dev`'s hot module replacement ([13-tooling.md](13-tooling.md#commands)).

## The panel

The VS Code and JetBrains plugins ship the same panel (both are webviews over `k preview`):

- **It follows the editor.** Opening a `.k` file with previews opens the panel beside it (a setting turns this off), and moving the cursor into a `preview` selects it. Each `preview` also gets a **▶ Preview** CodeLens.
- **Switcher:** the file's previews by name, in source order.
- **Controls,** one per parameter, chosen by type: `Bool` is a toggle, an enum is a dropdown of its cases, `Int` and `Double` are number fields with steppers, `String` is a text field, and any other type with a literal syntax (`Money`, `Duration`, `Date`) is a text field parsed by that type's literal parser and outlined when the text doesn't parse. A parameter of another type is an error at the declaration, naming the supported types. **Reset** restores the defaults and remounts.
- **Environment bar,** driven through the existing test doubles, so it changes exactly what a test can change: light, dark or forced colours and reduced motion (`TestPreferences`), viewport presets for phone, tablet and desktop plus a custom size (`TestViewport`), locale (the `metadata` locale the preview renders with), offline (`TestConnectivity`), and **Hold loading**, which keeps every `@Load` pending so `loading` shows. A `failed` state is staged with a fake that throws.
- **Grid:** every preview of the file at once, each in its own containers, all under the same environment bar. Controls are per preview, in a popover on each cell.

The panel shows only the preview. It doesn't map elements back to source or step the `TestClock`. Both are later items.

## `k check`

- **Previews are type-checked like any code,** in every command, so a preview that no longer compiles fails `k check`.
- **`k check` mounts every preview once** with its default parameters, in the simulated tab `k test` uses, and waits for it to settle. A preview whose setup throws, whose view fails to mount, or whose page ends in a guard redirect it didn't sign in for is reported as **K7520 `preview-failed`**, at the `preview` keyword, with the error. Like tests, this runs only in `k check`, never in the language server, and `--no-tests` skips it.
- **There are no image snapshots**, as there are no snapshot tests ([21-testing.md](21-testing.md#rules)). Tests can't render a preview. A test that wants the same state writes its own setup, so assertions don't depend on code edited casually for looks.

## Built so far

**Round 1 (2026-11-24): the compiler and `k check`.** The syntax (parser, formatter, TextMate grammar), the
`view-previews` row of `k_sema::features`, the checks, the preview bundle and `k check`'s mounts are built
([sema.md §12.14](../compiler/sema.md#1214-tests), [codegen.md §21.3](../compiler/codegen.md#213-the-preview-bundle-entrypreviews));
`k preview` and the editor panel are next, on `k_sema::Analysis::previews` and `k_codegen::Entry::Previews`.
`examples/notes` has three previews (`NoteCard.k`, `Notes.k`). Decided while building it, where the
decision above left a choice open:

- **Codes.** K7504 `preview-param-without-default`, K7505 `preview-param-type`, K7506
  `second-unnamed-preview`, K7507 `duplicate-preview-name`, K7508 `preview-not-a-view` (the last line is
  an expression of another type) and K7509 `preview-without-view` (an empty body, or one ending in another
  statement); K7520 `preview-failed`. Two codes for the two naming errors, so each message says which.
- **Which types have a control:** `Bool`, `Int`, `Double`, `String`, an enum whose cases all have no
  payload, and, as "a type with a literal syntax", `Money`, `Duration` and any type with
  `static func parse(_ text: String) -> T?` (`Date`, `Url`, `Uuid`, `EmailAddress`, `TimeOfDay`, `Decimal`,
  …). An optional (`Int?`) has none yet: its control would need a "none" state, which is a later item.
- **`use` rules and `@Inject` lines may be written anywhere in the body**, as test-group members are;
  they apply to the whole preview, before its setup runs. The examples put them first.
- **A layout's content is written by the preview** (`Frame { "Content" }`): a layout's `content` slot is
  required, so a bare `Frame()` doesn't type-check, and an automatic placeholder for it isn't built.
- **"A page that ends in a guard redirect"** is checked before the page renders: the page's guards
  (`Authenticated`, `Role`) against the session the setup left, as the router checks them before
  navigating. A refused guard fails the preview (K7520, naming the guard) rather than rendering the
  sign-in page.
- **A preview is test code by being in its file's preview scope**, a test file of the module over the
  same text: lookups, fixtures, fakes and `k.test` work unchanged, and the declaration gets an internal
  name (`preview#NoteCard#2310`) so the resolutions of its `@Inject` fields name their owner. A preview
  may also be written in a `.test.k` file or a `testing` module, which are test code already.
- **`k check`'s summary counts the mounts** (`ran 89 tests, mounted 3 previews`), and like the tests they
  run only in a project with a `k.toml` and no errors.

**Round 2 (2026-11-24): `k preview` and the language server's part.** `k preview [path]` serves the
index, the panel (switcher, controls, environment bar, grid), live rebuilds and the editor protocol
([docs/compiler/preview.md](../compiler/preview.md)); `k lsp` puts a **▶ Preview** CodeLens on each
`preview` keyword (command `kobra.preview.open`, argument `{uri, index, label}`) and lists previews in the
outline. The VS Code (round 3) and JetBrains (round 4) panels are next: a webview on
`/p/<file>/<index>?embed=1`, `k preview --editor` as the process, and the CodeLens command. Decided while
building it:

- **The editor talks to `k preview` directly**, over JSON lines on its standard input and output, with
  its own overlay of the unsaved buffers, rather than `k lsp` forwarding them. The plugin has the buffers
  already, the language server stays free of a second server's lifetime and ports, and the protocol is
  four messages each way. `k lsp`'s part is only the CodeLens.
- **A mount is a server of its own** (`k_server::Server`, one worker, a fresh process and in-memory
  database), and the browser loads the preview's frame from it: the frame's origin isolates its document,
  styles and theme, its requests go over the real wire format, and "fresh containers per mount" holds for
  the server side too. A grid of *n* previews runs *n* small servers.
- **The setup runs in the browser and reaches its server-side `@Inject` fields through a synchronous
  bridge** to the mount's isolate (`examples/notes`' `"signed in"` calls `SessionManager.signIn` through
  it). Values cross as tagged JSON or as handles; a browser object that is neither can't be passed. The
  alternatives (running the setup on the server and shipping the view, or the server in the browser) lose
  the real browser or the real database.
- **A page whose guards refuse the setup's session shows `k check`'s message** ("the page requires a
  signed-in user … sign in with `SessionManager` in the setup") in the frame and under it, rather than the
  sign-in page the router would show. The decision above says the panel shows the redirect; showing the
  sign-in page would need the app's `@SignInPage` route in the preview bundle and makes the refusal look
  like the preview's view, while `k check` keeps reporting K7520 for the same preview. Showing the redirect
  target stays possible later.
- **Control values live in the server**, per file and preview label, so a reloaded page or webview keeps
  them; an enum's value is its case's name and a parsed type's its text, parsed in the frame by the type's
  parser (a `Money` or `Duration` literal, or `parse`), and text that doesn't parse outlines the field and
  mounts the default.
- **The environment bar changes what a test can change** (`TestPreferences`, `TestViewport`,
  `TestConnectivity`), plus what the real browser needs to show it: the stylesheets' preference media
  rules are forced to match, the frame is sized to the viewport, the locale sets the document's `lang`, and
  **Hold loading** keeps the frame's requests pending.
- **The panel is plain HTML, CSS and JavaScript served from the `k` binary**, with no build step, as the
  rest of the toolchain has no npm ([13-tooling.md](13-tooling.md)).

**Round 3 (2026-11-24): the VS Code panel.** `editors/vscode/preview/`: `client.js`, the client of
`k preview --editor` with only Node's built-ins (the process, its restarts, the buffers, what `k` last
said), and `index.js`, the VS Code side (the **Kobra Preview** webview, the status bar item, the commands
`kobra.preview.open`, `kobra.preview.toggleGrid` and `kobra.preview.revealError`, the setting
`kobra.preview.autoOpen`). Tested with `node --test` in `editors/vscode`, against a fake `k preview`, a stub
of the `vscode` module and, when a `k` is built, the real one on the `previewing` fixture; `ci.yml`'s
`vscode` job runs them. Decided while building it:

- **One process per project that enables previews**, started the first time a `.k` file of it is open:
  the project is the nearest `k.toml` at or above the file (not above the workspace folder), and only one
  whose text names `view-previews` gets a process, since a preview anywhere else is K8108. A workspace with
  several apps runs one per app that previews. The process is `k.server.path` with `k.server.extraEnv`, the
  language server's settings, and a change to them restarts both.
- **The client keeps the open buffers** and sends them to each new process after its `ready`, so a
  restart loses nothing. A process that stops is restarted after 0.5, 1, 2, 5, 10, then every 30 seconds;
  six stops in a row, each within a minute of its start, give up with an error message naming `k`'s last
  line on standard error (an old `k` without `preview`, say).
- **The editor sends `select` for every cursor position at or after a file's first `preview` keyword**
  (throttled to one per 150 ms, the latest position), and `k` decides whether it is inside a declaration.
  The `previews` message has only the keywords' ranges, and `k` already tells the pages only when the
  selection changes, so a range check in the editor would duplicate `k`'s with less information.
- **Switching files switches the loaded page by message** (`{kpreview: "select", file, index}`) when it is
  the same process, keeping the environment bar; a different project, the grid toggle, a theme change and
  a restarted process load the page's URL again (the page reads `theme` and `mode` only from its URL, and
  keeps the environment bar in its session storage across the reload).
- **Closing the panel stops the automatic opening** until a command or a CodeLens opens it again (until
  the window reloads otherwise): reopening a panel the user has just closed on the next file switch would
  fight them. `kobra.preview.autoOpen` is in the `kobra.` namespace the CodeLens command already uses,
  while the language server's settings keep their `k.` names.
- **The page is reached through `vscode.env.asExternalUri`**, which forwards the port in a remote window
  and is the identity locally, and the webview's content security policy allows frames from that origin
  only, scripts only with a nonce. `portMapping` isn't used: it maps the webview's own requests, not an
  iframe's. The page is kept while the panel is hidden (`retainContextWhenHidden`), so its mounts and
  controls survive switching tabs. The extension runs where the workspace is (`extensionKind:
  workspace`), as `k` does.
- **The status bar item shows the error of the project the panel shows** (else the active file's), as
  "Preview: error at line N", and clicking it opens the file at the line and column; `built` hides it. It
  is a plain status bar item rather than a diagnostic: the language server already reports the same
  error, and a second diagnostic would duplicate it in the Problems view.
- **The extension's version doesn't change**: it is the toolchain's
  ([48](48-releases-and-versioning.md#the-vs-code-extension)), `0.1.0-rc.1`, set by the release.
- **No new npm dependency.** The panel is a webview with an iframe and a dozen lines of inline script; the
  tests use Node's built-in runner, so `npm install` is still needed only to package the extension.
- **Not built:** in VS Code for the Web (a Codespace in the browser) the frames don't load, because each
  mount serves its frame from `127.0.0.1` on a port of its own, which only the panel's page is forwarded
  for; serving the frames through the page's port, or forwarding each mount's port, is a later item. In a
  desktop remote window they show once VS Code forwards the mount's port to the same number.

**Round 4 (2026-11-24): the JetBrains panel.** `editors/intellij` has a **Kobra Preview** tool window
(right side) over `k preview --editor --no-open <root>`, one process per project root, and runs the
CodeLens's `kobra.preview.open` ([editors/intellij/README.md](../../editors/intellij/README.md)). Decided
while building it:

- **The CodeLens, not a gutter marker.** The platform's LSP client shows `k lsp`'s **▶ Preview** lens as a
  code vision hint, and the plugin runs the command itself through the LSP API's customization
  (`LspCodeLensSupport.codeLensClicked`, and `LspCommandsSupport.executeCommand` for the same command
  reached another way) rather than sending it back to the server. A run-style gutter icon would be more
  JetBrains-like, but the plugin's PSI is one token per file, so a `LineMarkerProvider` has no `preview`
  element to hang it on, and a second marker beside the lens would show the same action twice.
- **A process starts lazily, per root:** when a `.k` file opens whose text has a line starting with
  `preview` (a cheap test, before `k` has said anything), under the nearest `k.toml`, if that `k.toml`
  names `view-previews` (as the VS Code panel checks). A workspace's apps each get their own. It is sent every open buffer of its root on (re)start, every edit (the whole text,
  as the protocol has it), `didClose`, and, throttled to one per 150 ms, `select` for a cursor at or after
  a file's first `preview` keyword: `k` decides whether the cursor is inside a declaration, since the
  `previews` message gives only the keyword's offsets.
- **Restarts back off** 1 s, doubling to 30 s, starting over after a minute's run; after six restarts in a row that each ran under a minute
  the tool window says so and offers **Restart k preview**. The project's close sends `shutdown`.
- **The page reloads only for a new file, grid mode, theme or process**; moving between previews of the
  shown file is a protocol `select`, which `k` relays to the open page, so the panel's environment bar and
  scroll stay. The editor's paths are matched to `k`'s URIs by real path, since `k` canonicalizes its root
  (`/tmp` is `/private/tmp` on macOS).
- **The error is the tool window's header**, red text above the page, cleared by `built`, rather than a
  status bar widget or a notification: it is next to the preview it explains, and the page's own badge
  says the same. A failure to start or a give-up shows there too.
- **The browser is JCEF behind an optional dependency** (`com.intellij.modules.jcef`, a plugin of its own
  in 2026.2) and a small extension point, so the plugin installs where JCEF is missing; there, and where
  `JBCefApp.isSupported()` is false, the tool window links to the page in the system browser. The page's
  `theme` follows the IDE's (`JBColor.isBright()`) and reloads on a theme change.
- **Auto-open** is a setting (Settings | Languages & Frameworks | Kobra, on by default); the tool window
  shows once per selection of a file with previews, without taking focus, so closing it sticks until
  another file is selected.
- **Tests without the IDE's UI:** the protocol, the model (URLs, the cursor's preview, normalized paths),
  the backoff and throttle, and the session's start, restart and shutdown run against a fake `k` over
  pipes; in the headless IDE, the tool window's registration and the CodeLens click starting a fake `k`
  for the file's root. The JCEF page itself isn't tested (CI has no display).

## Rejected

- **Previews in sibling `Name.preview.k` files** (as `.test.k` is). This keeps production files free of test-only code, but Swift's inline placement keeps the preview where the view is edited, which is the point. Scoping preview bodies as test-only code gets the fixtures without a second file.
- **The `k dev` environment** (real services, `.env`, the development database). Previews would break as the data changes and couldn't show states the data doesn't contain.
- **Controls derived from the component's `@Prop`s.** It fails on non-literal types, slots and events, and makes the public API the preview API.
- **Serving previews from `k dev`.** It mixes the hermetic wiring with the development wiring and is tied to the save-and-rebuild cycle.
