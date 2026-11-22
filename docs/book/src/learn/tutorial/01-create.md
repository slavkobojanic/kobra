# 1. Create the project

In this step you'll create the Shelf project, start it in the browser, and change its start page to say
what the app is for. On the way you'll meet the four things every step after this one builds on: the
project, a page, its view and `k check`.

The finished step is in the repository as `examples/learn/shelf/01-create`.

## Create it

`k new` creates a project in a new directory. Run it where you keep your code:

```console
$ k new shelf
Created the app `shelf` in shelf/. Start it with:

    cd shelf
    k dev
```

Do what it says:

```bash
cd shelf
k dev
```

`k dev` is the development server. It builds the app, starts it and prints its address:

```text
serving on http://localhost:8080
```

Open `http://localhost:8080` in the browser. You'll see a heading, "Hello from Kobra", a count and a button
that adds one to it. Leave `k dev` running in this terminal: it rebuilds the app every time you save a
file, and you reload the browser to see the change. Stop it with Ctrl-C when you're done.

## What `k new` wrote

A project is a directory with a `k.toml` file in it. `k new` wrote these files:

```text
shelf/
  k.toml                 the project manifest: its name and the Kobra version it needs
  App.k                  the app declaration: settings every page shares
  .gitignore
  src/
    home/
      Home.k             the start page
      Home.test.k        a test for it
    theme/
      AppTheme.k         design tokens: colors, spacing, fonts
```

Your code goes in `src/`. Every directory below it is a module, a group of declarations that belong
together, and the module's name is the directory's name: `src/home/` is the module `home`. Each file holds
at most one public declaration, named like the file, so `Home.k` holds `Home`. You'll never need to say
where a file is: Kobra finds every page, component and theme from its declaration.

Open `src/home/Home.k`. It starts with `public page Home at "/"`: a page is a part of the app with its
own address, and this one is at `/`, the start page. Inside it, `metadata { title("Shelf") }` sets the
title the browser shows in its tab, and `view { … }` describes what the page shows. The rest of the file,
the `@State` line and the `style` block, comes up in later steps.

## Change the start page

Replace the contents of `src/home/Home.k` with this page:

```k
// examples/learn/shelf/01-create/src/home/Home.k
/// The app's start page: the reading list.
public page Home at "/" {
    metadata {
        title("Shelf")
    }

    view {
        main {
            heading(level: 1) { "Shelf" }

            paragraph { "The books you want to read, and the ones you've read." }
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

Save it and reload the browser: the page now has the heading "Shelf" and a sentence under it.

A view is a tree of elements. Each element is written as its name, then its contents in braces:

- `main` holds the page's main content. Elements that hold other elements nest inside its braces.
- `heading(level: 1)` is the page's top heading. `level: 1` is a named argument: in Kobra you write the name of
  each argument, so a call says what every value means.
- `paragraph` is a paragraph of text. Text is written as a string, in double quotes.
- `.class(.page)`, on the line after `main`'s closing brace, applies to `main`. It gives it the `page`
  class from the `style` block at the bottom of the file, which keeps the text in a narrow column.

The line that starts with `///` is a documentation comment. It describes the declaration under it, and
your editor shows it when you hover over `Home`.

## Check it

`k check` is the command you'll run most. It reads the whole project, tells you about every mistake it
finds, and then runs the project's tests. Open a second terminal in the `shelf` directory, so `k dev`
keeps running in the first one, and run it:

```text
error[K7510]: button(named: "Add one") found nothing
 --> src/home/Home.test.k:4:10
  |
4 |     test "the button counts clicks" {
  |          ^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: in test `Home › the button counts clicks`

checked 4 files, ran 1 test: 1 error(s), 0 warning(s)
```

It fails, because `k new` also wrote a test, `src/home/Home.test.k`, and that test clicks the button you
just removed. A test is Kobra code that renders the page and states what must be true about it; step 3
explains how to write them. For now, replace the contents of `src/home/Home.test.k` with a test of the
new page:

```k
// examples/learn/shelf/01-create/src/home/Home.test.k
import k.test.render

test Home {
    test "the start page shows the app's name" {
        let screen = render(Home())

        expect screen.heading(named: "Shelf").isVisible
    }
}
```

It renders the page and expects a heading named "Shelf" on it. Run `k check` again:

```console
$ k check
checked 4 files, ran 1 test: no problems
```

`k check` is the one command that says whether the project is right: run it before you commit, and it's
what a build server runs too.

**Try it:** misspell an element on purpose. In `src/home/Home.k`, change `heading(level: 1)` to
`heding(level: 1)` and save. The terminal running `k dev` says:

```text
change detected; rebuilding
error: the project has 1 error(s); run `k check .` to see them
the last good build keeps serving
```

The browser keeps showing the last page that built, so a typo never leaves you with a broken page. Run
`k check` in the second terminal to see the error:

```text
error[K3101]: `heding` is not a built-in element
 --> src/home/Home.k:9:13
  |
9 |             heding(level: 1) { "Shelf" }
  |             ^^^^^^
  |
  = fix: Replace `heding` with `heading`

checked 4 files: 1 error(s), 0 warning(s)
```

Every error has a code (`K3101`), the file, line and column it's at, and the line itself, with the
mistake underlined. Many, like this one, end with a `fix:` that says what to write instead. Your editor
shows the same errors as you type, if you've set it up ([Editor setup](../../getting-started/editor-setup.md)).
Change it back to `heading` and save.

## Recap

In this step you:

- created a project with `k new` and ran it with `k dev`, which rebuilds on every save;
- saw how a project is laid out: `k.toml`, `App.k`, and one module per directory below `src/`;
- wrote a page, `Home`, at the address `/`, whose `view` is a tree of elements;
- ran `k check`, which reports every mistake with its place and a fix, then runs the tests.

## Next

- [2. Show a list of books](02-list.md): describe a book as a type, and show a list of them.
- [Project layout](../../getting-started/project-layout.md) and
  [Everyday commands](../../getting-started/commands.md) cover the files and the commands in full.
