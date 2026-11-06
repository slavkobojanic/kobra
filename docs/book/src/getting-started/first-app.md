# Your first app

In this chapter you'll create a K app, run it, change it, test it and build it for production. It
takes about five minutes.

> **Proposal.** `k new` and the skeleton it generates follow the proposal in
> `docs/design/proposals/project-creation.md`.

## Creating a project

```bash
k new hello
cd hello
```

`k new` creates a directory with everything an app needs and nothing it doesn't:

```text
hello/
  k.toml                 the project manifest
  App.k                  the app declaration
  .gitignore
  src/
    home/
      Home.k             the start page
      Home.test.k        a test for it
    theme/
      AppTheme.k         design tokens: colors, spacing, fonts
```

There are no templates to choose from and nothing to configure. Every K project starts from the same
skeleton, and the [project layout](project-layout.md) is fixed.

## Running it

```bash
k dev
```

`k dev` compiles the app, starts the development server and prints its address, by default
`http://localhost:8080`. Open it in a browser: you'll see a heading, a counter and a button.

`k dev` keeps running. When you save a file it rebuilds what changed and reports any compile errors in
the terminal.

> **Planned.** State-preserving hot module replacement (HMR) is designed but deferred. Today `k dev`
> rebuilds on save and the page reloads. With HMR, edits will apply without a reload, and component
> state will be preserved, which is possible because K state is always declared explicitly with
> `@State`.

## Reading the start page

Open `src/home/Home.k`:

```k
/// The app's start page.
public page Home at "/" {
    @State var count: Int = 0

    metadata {
        title("Hello")
    }

    view {
        main {
            heading(level: 1) { "Hello from K" }

            paragraph { "Count: {count}" }

            button { "Add one" }
                .on(click: { count += 1 })
        }
        .class(.page)
    }

    style {
        .page {
            maxWidth: 40rem;
            margin: 0 auto;
            padding: .lg;
            font: .body;
        }
    }
}
```

Even without reading the rest of this book, most of it is readable:

- `public page Home at "/"` declares a page. Pages get their URLs from the compiler; this one names
  its URL explicitly because it's the site root.
- `@State var count: Int = 0` is reactive state. Writing it updates exactly the DOM nodes that read
  it.
- `view { … }` describes the UI. Lowercase names (`main`, `heading`, `paragraph`, `button`) are
  built-in elements; text is just a string inside a block.
- `.on(click: { count += 1 })` handles an event with a lambda.
- `.class(.page)` refers to the `.page` class in the `style` block. Styles are scoped to the page, and
  `.lg` and `.body` are tokens from `AppTheme`.
- `metadata { … }` sets the document title.

## Making a change

Change the button and the paragraph:

```k
paragraph { "You clicked {count} times." }

button { "Click me" }
    .on(click: { count += 1 })
```

Save, and the browser shows the change. Now make a mistake on purpose: change `.class(.page)` to
`.class(.pgae)`. `k dev` reports a compile error, because `.pgae` isn't a class in this page's
`style` block. Class names, like page names, element names and property values, are checked.

## Testing it

`Home.test.k` sits next to `Home.k`:

```k
test Home {
    test "the button counts clicks" {
        let screen = render(Home())

        screen.button(named: "Add one").click()

        expect screen.text("Count: 1").isVisible
    }
}
```

The test finds the button the way a screen reader would, by its role and name. After your change it
no longer matches, so run the tests and see:

```bash
k test
```

`k test` reports the failing query and what it found. Update the test to use `"Click me"` and
`"You clicked 1 times."`, and run it again. Use `k test --watch` to re-run affected tests on every
save.

## Checking everything

```bash
k check
```

`k check` is the one command CI runs. It type-checks the whole project, checks formatting, runs the
tests, and compares the lockfiles with the code to catch breaking changes. If it passes, the project is
ready to ship.

## Building for production

```bash
k build
```

`k build` compiles the production build into `dist/`:

```text
dist/
  app        a single, self-contained server executable
  public/    the hashed client assets (also embedded in dist/app)
```

`dist/app` contains the server bundle, the JavaScript runtime and every client asset. Running the app
in production means running that one file:

```bash
K_PUBLIC_ORIGIN=http://localhost:8080 dist/app
```

`K_PUBLIC_ORIGIN` tells the server the address users reach it at; production builds require it. The
[Configuration and deployment](../apps/deployment.md) chapter covers configuration, containers and
health checks.

## Where to go next

- [Project layout](project-layout.md) explains every file in a K project.
- [Everyday commands](commands.md) covers `k dev`, `k build`, `k test` and `k check` in more depth.
- [A tour of K](../tour.md) builds a small full-stack feature, from the database to the button.
