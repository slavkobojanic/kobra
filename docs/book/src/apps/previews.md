# Preview views

A **`preview`** renders a view in a chosen state, written in the file of the view it shows. `k check`
mounts every preview once, so a preview that stops rendering fails the check. The editor panel and the
preview server that show previews beside your code come in a later release. The design is in `docs/design/57-previews.md`.

> **Preview** (view-previews). Previews are a preview feature: they are built, but may still change in any
> minor release. A project opts in with `preview = ["view-previews"]` in the `[package]` section of its
> `k.toml`, as `examples/notes` does; without it each `preview` is an error (`K8108`) whose quick fix adds
> the line.

## Declaring a preview

A preview is a top-level declaration, after or between the file's other declarations. It has an optional
name, optional parameters, and a body that ends with the view it renders:

```k
// examples/notes/src/notes/NoteCard.k
preview (title: String = "Groceries", pinned: Bool = false) {
    @Inject let clock: TestClock

    NoteCard(note: summary(title: title, body: "Milk and eggs", tags: ["home", "weekly"], isPinned: pinned, updatedAt: clock.now))
}

preview "without tags" {
    @Inject let clock: TestClock

    NoteCard(note: summary(title: "Ideas", updatedAt: clock.now))
}
```

- **The name** labels the preview in the panel's switcher. An unnamed preview is labelled after the view
  it renders (`NoteCard`). A file has at most one unnamed preview (`K7506`), and two previews of a file
  can't share a name (`K7507`).
- **Parameters** are the preview's state, and the panel shows a control for each: a toggle for a `Bool`,
  a number field for an `Int` or a `Double`, a text field for a `String`, a dropdown for an enum whose
  cases have no payload, and a text field parsed by the type's own parser for a type with a literal
  syntax (`Money`, `Duration`, `Date`, `Url`, …). Every parameter has a default, because the preview
  renders before anyone touches a control (`K7504`); another type is `K7505`.
- **A `@Binding` takes a parameter:** `Stepper(value: bind value)` writes back to the parameter, which
  moves its control. A literal can't be bound, so a fixed-state preview of a component with a binding
  also uses a parameter.
- **The body** is setup, then exactly one view: a component, a layout given its content, or a page. A body
  that ends with anything else is `K7508`, and one that renders nothing `K7509`.

## A preview is test code

A preview's body is checked as test code of its module, even though it sits in a production file:

- it sees what the module's tests see: fixtures from the public extensions of `testing` modules
  (`AccountId.ada`), helpers declared in the module's `.test.k` files (`summary(…)` above), and the
  `k.test` doubles (`TestClock`) without an import;
- it may bind fakes with `use FakeX for X`, inject them and the test doubles with `@Inject let`, and run
  statements, `await` and `try` included, as a test group does ([Testing](testing.md#test-files-and-groups));
- a production file may import a `testing` module only for its previews: a use of the imported name
  anywhere else in the file is the usual test-only import error (`K1104`), at the use.

Every preview runs hermetically, as a test does ([Which side a test runs on](testing.md#which-side-a-test-runs-on)):
a fresh server and browser container with the `k test` defaults, an in-memory database with every
migration applied, and one session. It never reads `.env`, the development database or the network.
Previews never reach `k build`, `k dev`, `k run` or `k test`: the compiler drops them, and the `testing`
imports only they use, from every build.

## Previewing a page

A page renders inside its layouts, as the router shows it after navigating there. Its guards apply, so a
page that requires a signed-in user needs a setup that signs in:

```k
// examples/notes/src/notes/Notes.k
preview "signed in" {
    @Inject let sessions: SessionManager

    await sessions.signIn(userId: AccountId.ada)

    Notes()
}
```

## `k check` mounts every preview

After the tests, `k check` mounts each preview once with its default parameters, in the simulated
browser tab the tests use, and waits for it to settle. A preview whose setup throws, whose view fails to
mount, or whose page's guards refuse the session its setup left is reported as `K7520` at its `preview`
keyword:

```text
error[K7520]: the page requires a signed-in user and the preview's setup signs nobody in, so the router would redirect (sign in with `SessionManager` in the setup)
  --> src/shop/Account.k:11:1
   |
11 | preview "signed out" {
   | ^^^^^^^
   |
   = note: in preview `signed out`, mounted with its default parameters
```

`--no-tests` skips the mounts with the tests, and the language server never runs them. Tests can't render
a preview, and there are no image snapshots: a test that wants the same state writes its own setup.
