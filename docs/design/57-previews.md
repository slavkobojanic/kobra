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
- **From an editor,** the plugin starts it and points a webview at it, and `k lsp` sends it the open buffers, so a preview updates as you type, debounced, whenever the file type-checks. While the file has errors, the last good render stays, under a badge naming the first error. Control values are kept across rebuilds by parameter name and type. A renamed or retyped parameter goes back to its default.
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

## Rejected

- **Previews in sibling `Name.preview.k` files** (as `.test.k` is). This keeps production files free of test-only code, but Swift's inline placement keeps the preview where the view is edited, which is the point. Scoping preview bodies as test-only code gets the fixtures without a second file.
- **The `k dev` environment** (real services, `.env`, the development database). Previews would break as the data changes and couldn't show states the data doesn't contain.
- **Controls derived from the component's `@Prop`s.** It fails on non-literal types, slots and events, and makes the public API the preview API.
- **Serving previews from `k dev`.** It mixes the hermetic wiring with the development wiring and is tied to the save-and-rebuild cycle.
