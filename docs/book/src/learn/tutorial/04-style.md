# 4. Make it look right

In this step you'll give Shelf its look: a few named colors and sizes in the theme, styles for the list
and the form, and a dark version that follows the system's setting. Nothing about what the app does
changes; it only looks finished.

Start from the project you finished in [step 3](03-form.md), with `k dev` running. The finished step is in
the repository as `examples/learn/shelf/04-style`.

## Name the colors and sizes

A design uses the same few colors and sizes everywhere. In Kobra they're written once, as tokens in the app's
theme, and every style refers to them by name. `k new` wrote a theme with three tokens, and the page has
used two of them since step 1: `padding: .lg` and `font: .body`. Replace `src/theme/AppTheme.k` with a
fuller one:

```k
// examples/learn/shelf/04-style/src/theme/AppTheme.k
/// The app's design tokens: colors, spacing and fonts, with a dark variant
/// that follows the system's dark mode.
public theme AppTheme {
    color primary: Color = #2563EB
    color onPrimary: Color = #FFFFFF
    color text: Color = #1C1917
    color muted: Color = #78716C
    color background: Color = #FAFAF9
    color surface: Color = #FFFFFF
    color border: Color = #E7E5E4

    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px

    radius md: Radius = 8px

    font body: Font = Font(family: "system-ui", size: 16px)

    variant dark matches .prefersDark {
        primary = #93C5FD
        onPrimary = #1C1917
        text = #F5F5F4
        muted = #A8A29E
        background = #1C1917
        surface = #292524
        border = #44403C
    }
}
```

- Each line declares a token: its kind (`color`, `space`, `radius`, `font`), its name, its type and its
  value. `color muted: Color = #78716C` makes `.muted` a color any style can use.
- The kinds are different types. A `Spacing` is a length for gaps and padding, a `Radius` for rounded
  corners, so a style can't use a color where a size belongs.
- `text` and `background` are the colors of the page itself. Kobra uses them for the page's text and
  background without a style of yours.
- `variant dark { … }` is another set of values for tokens the theme already has. It can't add new
  ones. `matches .prefersDark` turns it on when the system is set to dark mode, so the app follows the
  computer or phone it runs on.

The page already looks different when you save: the background is a warm off-white, and the text is
nearly black.

## Style the list

A component's styles go in a `style` block next to its `view`, as the page's do. Give each book a row of
its own, with the author in a quieter color. Replace `src/books/BookRow.k`:

```k
// examples/learn/shelf/04-style/src/books/BookRow.k
/// One book in the reading list: its title, and who wrote it.
public component BookRow {
    /// The book to show.
    @Prop let book: Book

    view {
        item {
            emphasis { book.title }

            text { " by {book.author}" }
                .class(.author)
        }
        .class(.row)
    }

    style {
        .row {
            padding: .md;
            borderBottom: 1px solid .border;

            &:last-child { borderBottom: none; }
        }

        .author { color: .muted; }
    }
}
```

- The style block is CSS, with one change: names are written in camelCase, `borderBottom` for
  `border-bottom` (keyword values too: `spaceBetween` for `space-between`). Each declaration ends with `;`.
- After a property, `.md` and `.border` are tokens from the theme. At the start of a rule, `.row` is a
  class of this block.
- `&:last-child { … }` is a nested rule for the last row: `&` stands for `.row`. The last book has no line
  under it, because the list gets a border of its own.
- Styles are scoped: the compiler gives `.row` a name only this component uses, so another component's
  `.row` can never change these rows.

Then give the list that border, and a background, in `src/home/Home.k`:

```k
// examples/learn/shelf/04-style/src/home/Home.k
// ...
        .books {
            listStyle: none;
            padding: 0;
            border: 1px solid .border;
            borderRadius: .md;
            backgroundColor: .surface;
        }
// ...
```

**Try it:** misspell a class. In `BookRow.k`, change `.class(.author)` to `.class(.auther)` and run
`k check`:

```text
error[K3203]: the style block has no class `auther`
  --> src/books/BookRow.k:11:25
   |
11 |                 .class(.auther)
   |                         ^^^^^^
   |
   = fix: Replace `auther` with `author`

warning[K3204]: class `author` is never used
  --> src/books/BookRow.k:24:10
   |
24 |         .author { color: .muted; }
   |          ^^^^^^

checked 8 files: 1 error(s), 1 warning(s)
```

A class that isn't in the style block is an error, and one the view never uses is a warning, so styles
can't quietly drift away from the view. Change it back.

## Style the form

The form's fields and button get classes of their own, in `src/books/AddBookForm.k`:

```k
// examples/learn/shelf/04-style/src/books/AddBookForm.k
// ...
    view {
        form {
            textField(value: bind draft.title)
                .label("Title")
                .class(.field)

            textField(value: bind draft.author)
                .label("Author")
                .class(.field)

            button { "Add book" }
                .type(.submit)
                .class(.add)
        }
        .class(.form)
        .on(submit: draft.handle(submit))
    }

    style {
        .form {
            display: grid;
            gap: .sm;
        }

        .field {
            padding: .sm;
            border: 1px solid .border;
            borderRadius: .md;
            backgroundColor: .surface;
            color: .text;
        }

        .add {
            justifySelf: start;
            marginTop: .sm;
            padding: .sm .md;
            border: none;
            borderRadius: .md;
            backgroundColor: .primary;
            color: .onPrimary;
        }
    }
}
```

- `.class(.field)` on a text field styles the box you type in. Its label stays as it is.
- `display: grid` puts each label, field and button on a line of its own, with a `.sm` gap between them.
- `justifySelf: start` keeps the button as wide as its text, instead of the whole form.

Save and reload: the labels sit above their fields, and Add book is a blue button.

**Try it:** put a color where a size belongs. In `BookRow.k`, change `padding: .md;` to
`padding: .primary;`:

```text
error[K3202]: this doesn't fit `padding`: <length-percentage>{1,4}
  --> src/books/BookRow.k:18:22
   |
18 |             padding: .primary;
   |                      ^^^^^^^^

checked 8 files: 1 error(s), 0 warning(s)
```

Every value in a style is checked against what its property accepts, here one to four lengths or
percentages. A color isn't one, so the mistake is caught before the browser would have ignored it. Change
it back to `.md`.

## See the dark variant

Set your system to dark mode (on macOS, System Settings › Appearance; on Windows, Settings ›
Personalization › Colors), and the page turns dark at once, without a reload. In Chrome you can try it
without changing the system: open DevTools, press Ctrl+Shift+P (Cmd+Shift+P on a Mac), and run
**Emulate CSS prefers-color-scheme: dark**.

Every style you wrote uses tokens, so each one follows the variant without a change: the rows' lines,
the fields and the button all use the dark values. Kobra also tells the browser the page is dark, so its
scrollbars and built-in controls turn dark too.

**Try it:** add a token to the variant only. Add `accent = #FDE68A` as the last line of
`variant dark { … }`:

```text
error[K3301]: no theme declares the token `accent`
  --> src/theme/AppTheme.k:28:9
   |
28 |         accent = #FDE68A
   |         ^^^^^^

checked 8 files: 1 error(s), 0 warning(s)
```

A variant only changes tokens the theme declares, so a token can't exist in dark mode and be missing in
light mode. Remove the line.

## Check it

Styles change how the page looks, not what's on it, so the tests from step 3 still pass unchanged. Run
`k check`:

```console
$ k check
checked 8 files, ran 3 tests: no problems
```

## Recap

In this step you:

- declared the app's colors and sizes once, as tokens in its `theme`, each of its own kind;
- styled components with `style` blocks: CSS with camelCase property names, using tokens by name;
- saw that styles are scoped to their component, and that a missing or unused class is reported;
- added a `variant` that follows the system's dark mode, which every style follows through its tokens.

## Next

- [5. Save books on the server](05-server.md): keep the books in a database, so they survive a restart.
- [Styles and themes](../../apps/styles.md) covers styles, tokens and variants in full, including
  switching variants with a button.
