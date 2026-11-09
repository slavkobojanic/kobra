# Your first app

In this chapter you'll create a K app, run it, change it, test it and build it for production. It
takes about five minutes.

## Creating a project

> **Planned** (k-new). `k new hello` will create this skeleton for you. It isn't built yet, so for now you
> write the five files by hand. They are short, and writing them once shows what every K app is made
> of.

A K app is a directory with a manifest, an app declaration and a `src/` directory. Create it:

```bash
mkdir -p hello/src/home hello/src/theme
cd hello
```

```text
hello/
  k.toml                 the project manifest
  App.k                  the app declaration
  src/
    home/
      Home.k             the start page
      Home.test.k        a test for it
    theme/
      AppTheme.k         design tokens: colors, spacing, fonts
```

The manifest, `k.toml`, names the app and the toolchain version it needs:

```toml
[package]
name = "hello"
title = "Hello"
k = "0.1"
version = "0.1.0"
```

`App.k` declares the app. It holds only what the compiler can't infer; this one names the document's
language and nothing else:

```k
// App.k

/// A first K app.
public app Hello {
    metadata {
        locale(.english)
    }
}
```

`src/theme/AppTheme.k` declares the design tokens the styles use:

```k
// src/theme/AppTheme.k

/// The app's design tokens.
public theme AppTheme {
    color primary: Color = #2563EB

    space lg: Spacing = 24px

    font body: Font = Font(family: "system-ui", size: 16px)
}
```

The start page and its test follow below. The [project layout](project-layout.md) is fixed: every
K project has this shape, and there is nothing to configure. Add a `.gitignore` that excludes `dist/`,
`.k/` and `.env`.

## Running it

```bash
k dev
```

`k dev` compiles the app, starts the development server and prints its address, by default
`http://localhost:8080`. Open it in a browser: you'll see a heading, a counter and a button.

`k dev` keeps running. When you save a file it rebuilds the app. A build with errors is reported in the
terminal with their count (``error: the project has 1 error(s); run `k check .` to see them``), and the
last good build keeps serving.

> **Planned** (hmr). State-preserving hot module replacement (HMR) isn't in the first compiler releases.
> Today `k dev` rebuilds and restarts the server on save, and you refresh the browser to see the change.
> A build with errors is reported and the last good build keeps serving. With HMR, edits will apply
> without a reload, and component state will be preserved, which is possible because K state is always
> declared explicitly with `@State`.

## Reading the start page

Create `src/home/Home.k`:

```k
// src/home/Home.k

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
~page Changed at "/changed" {
~    @State var count: Int = 0
~
~    view {
paragraph { "You clicked {count} times." }

button { "Click me" }
    .on(click: { count += 1 })
~    }
~}
```

Save and refresh the browser to see the change. Now make a mistake on purpose: change `.class(.page)` to
`.class(.pgae)`. `k dev` reports that the build has an error, and `k check` shows it: `K3203`, because
`.pgae` isn't a class in this page's `style` block, with a fix that suggests `.page`. Class names, like page names, element names and property values, are checked.

## Testing it

Tests sit next to the code they test. Create `src/home/Home.test.k`:

```k
// src/home/Home.test.k
~import k.test.render

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

`k check` is the gate CI runs. It parses and type-checks the whole project, applies every static
rule of the language (layering, the server/client boundary, exhaustive matches, class and page
references, doc links), compares the lockfiles with the code to catch breaking changes, and then runs
every test, as `k test` does. `--no-tests` skips the tests for a quick type check. CI needs only
`k check`.

> **Planned** (k-fmt). The design adds the formatting check to `k check`. There is no formatter yet.

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

`K_PUBLIC_ORIGIN` tells the server the address users reach it at. Without it, `dist/app` assumes
`http://localhost:<port>`, which is right only on your own machine, so set it in every deployment. The
[Configuration and deployment](../apps/deployment.md) chapter covers configuration, containers and
health checks.

## Where to go next

- [Project layout](project-layout.md) explains every file in a K project.
- [Everyday commands](commands.md) covers `k dev`, `k build`, `k test` and `k check` in more depth.
- [A tour of K](../tour.md) builds a small full-stack feature, from the database to the button.
