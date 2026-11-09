# Quick start

On this page you'll build a tip splitter: you enter a bill, pick a tip and say how many people are
paying, and the page shows what each person pays. It takes about ten minutes, and on the way you'll use
the parts every K app has: state, a view, a component, styles, a test and a build.

You need the `k` command. If you don't have it yet, follow [Installation](../getting-started/installation.md)
first. The finished app is in the repository as `examples/learn/quick-start`.

## Create the project

`k new` creates a project in a new directory:

```console
$ k new tip
Created the app `tip` in tip/. Start it with:

    cd tip
    k dev
```

Do what it says:

```bash
cd tip
k dev
```

`k dev` builds the app, starts the development server and prints its address, `http://localhost:8080`.
Open it in the browser: you'll see a heading, a counter and a button. `k dev` keeps running and rebuilds
the app every time you save a file; reload the browser to see the change.

`k new` wrote these files:

```text
tip/
  k.toml                 the project manifest: its name and the K version it needs
  App.k                  the app declaration
  .gitignore
  src/
    home/
      Home.k             the start page
      Home.test.k        a test for it
    theme/
      AppTheme.k         design tokens: colors, spacing, fonts
```

Every directory below `src/` is a module, and a file holds at most one public declaration, named like
the file. The start page is `src/home/Home.k`.

## Keep the bill in state

Replace the contents of `src/home/Home.k` with this page:

```k
/// Splits a bill, with a tip, between the people at the table.
public page Home at "/" {
    @State var bill: String = ""
    @State var tip: Int = 15
    @State var people: Int = 2

    @Derived let billCents: Int = ((Double.parse(bill) ?? 0.0) * 100.0).rounded()
    @Derived let total: Money = Money(minorUnits: billCents * (100 + tip) / 100, currency: .usd)
    @Derived let share: Money = total.allocated(into: if people < 1 { 1 } else { people })[0]

    view {
        main {
            heading(level: 1) { "Tip splitter" }

            textField(value: bind bill)
                .label("Bill")

            numberField(value: bind people)
                .label("People")

            paragraph { "Each person pays {share.formatted(as: .currency)}" }
        }
    }
}
```

Save, reload the browser, type `50` as the bill, and the page says that each person pays $28.75.

- `@State var` declares state: a value the page owns that can change. The bill is the text you type,
  the tip a percentage and `people` a whole number.
- `@Derived let` declares a value computed from state. When the bill, the tip or the number of people
  changes, `total` and `share` follow, and so does the paragraph that shows `share`. You don't write
  any code to make that happen.
- `textField(value: bind bill)` is a text field that reads and writes `bill`. `bind` marks the places
  where a field is allowed to change your state. `numberField` does the same for a whole number.
- `Double.parse(bill)` reads the text as a number, and gives `nil` (no value) when the text isn't one,
  such as while the field is empty; `?? 0.0` says what to use instead.
- `Money` is an exact amount in a currency, kept in cents (`minorUnits`) rather than as a floating-point
  number, so it never rounds by accident. `allocated(into:)` splits it into parts that add up to the
  total exactly, and `formatted(as: .currency)` writes it as `$28.75`.
- `if` is an expression in K, so `if people < 1 { 1 } else { people }` is a value: nobody divides a bill
  between zero people.

## Pick the tip with buttons

The tip is fixed at 15% so far. Add three buttons above the `People` field, each setting the tip:

```k
~page TipButtons at "/tip-buttons" {
~    @State var tip: Int = 15
~
~    view {
box {
    button { "10%" }
        .on(click: { tip = 10 })

    button { "15%" }
        .on(click: { tip = 15 })

    button { "20%" }
        .on(click: { tip = 20 })
}
~    }
~}
```

`box` groups elements without giving them a meaning, and `.on(click: { … })` runs the code in braces
(a lambda) when the button is clicked. Click `20%`: the share changes at once.

## Make a component

The buttons work, but nothing shows which tip is in use, and the three of them repeat the same code.
K's standard library has no segmented control, so you'll build one: a component, a reusable piece of
the view. Create `src/home/TipOption.k`:

```k
// examples/learn/quick-start/src/home/TipOption.k
/// One tip percentage, as a button that shows whether it is the one in use.
public component TipOption {
    /// The tip this button offers, in percent.
    @Prop let percent: Int
    /// Whether this is the tip in use.
    @Prop let isSelected: Bool
    /// Fired with `percent` when the button is clicked.
    @Emits let chosen: Event<Int>

    view {
        button { "{percent}%" }
            .pressed(isSelected)
            .class([.option, .selected when isSelected])
            .on(click: { chosen.emit(percent) })
    }

    style {
        .option {
            padding: .sm .lg;
            border: 1px solid .primary;
            borderRadius: .sm;
            backgroundColor: transparent;
            color: .primary;
        }

        .selected {
            backgroundColor: .primary;
            color: white;
        }
    }
}
```

- `@Prop` declares an input: the page passes it, and the component can read it but not change it.
- `@Emits` declares an event, something that happened, and `chosen.emit(percent)` fires it. The page
  decides what to do about it; the component only reports the click.
- `.pressed(isSelected)` tells screen readers whether the button is the selected one, and
  `.class([.option, .selected when isSelected])` gives it the `selected` class only while it is.

Now use it in the page. Replace the three buttons with:

```k
// examples/learn/quick-start/src/home/Home.k
// ...
box {
    TipOption(percent: 10, isSelected: tip == 10)
        .on(chosen: { percent => tip = percent })

    TipOption(percent: 15, isSelected: tip == 15)
        .on(chosen: { percent => tip = percent })

    TipOption(percent: 20, isSelected: tip == 20)
        .on(chosen: { percent => tip = percent })
}
.class(.tips)
// ...
```

The page listens to `chosen` with the same `.on(…)` it used for the click, and the lambda gets the
event's value, the percentage. The page owns the state; the component shows it and reports clicks.

## Style it

The finished page, in `examples/learn/quick-start/src/home/Home.k`, also shows the total, and has a
`style` block:

```k
// examples/learn/quick-start/src/home/Home.k
// ...
paragraph { "Total with tip: {total.formatted(as: .currency)}" }

paragraph { "Each person pays {share.formatted(as: .currency)}" }
    .class(.share)
// ...
style {
    .page {
        maxWidth: 40rem;
        margin: 0 auto;
        padding: .lg;
        font: .body;
    }

    .tips {
        display: flex;
        gap: .sm;
    }

    .share { color: .primary; }
}
// ...
```

A `style` block belongs to its page or component: its classes apply only there, and `.class(.tips)`
names one of them. Style properties are written in camelCase (`maxWidth`, `backgroundColor`). Values
such as `.lg`, `.sm` and `.primary` are tokens of the app's theme, `src/theme/AppTheme.k`. The
skeleton has no `sm` spacing, so add it:

```k
// examples/learn/quick-start/src/theme/AppTheme.k
/// The app's design tokens: colors, spacing and fonts.
public theme AppTheme {
    color primary: Color = #2563EB

    space sm: Spacing = 8px
    space lg: Spacing = 24px

    radius sm: Radius = 4px

    font body: Font = Font(family: "system-ui", size: 16px)
}
```

**Try it:** misspell a class on purpose. Change `.class(.tips)` to `.class(.tpis)` and run `k check` in
a second terminal:

```text
error[K3203]: the style block has no class `tpis`
  --> src/home/Home.k:32:21
   |
32 |             .class(.tpis)
   |                     ^^^^
   |
   = fix: Replace `tpis` with `tips`

warning[K3204]: class `tips` is never used
  --> src/home/Home.k:53:10
   |
53 |         .tips {
   |          ^^^^

checked 5 files: 1 error(s), 1 warning(s)
```

Class names, like token names, element names and property values, are checked when the app compiles,
so a typo never reaches the browser. Your editor shows the same error as you type, if you set it
up ([Editor setup](../getting-started/editor-setup.md)). Change it back.

## Test it

The test `k new` wrote clicks the counter's button, which is gone. Replace `src/home/Home.test.k` with a
test of the tip splitter:

```k
// examples/learn/quick-start/src/home/Home.test.k
import k.test.render

test Home {
    test "each person pays their part of the bill and the tip" {
        let screen = render(Home())

        screen.textField(labeled: "Bill").fill("50")
        screen.button(named: "20%").click()

        expect screen.text("Total with tip: $60.00").isVisible
        expect screen.text("Each person pays $30.00").isVisible
    }
}
```

`render` shows the page as the browser would, and `screen` finds things on it the way a person using a
screen reader does: the field by its label, the button by its name. `expect` states what must be true.
Run every check, the tests included:

```console
$ k check
checked 5 files, ran 1 test: no problems
```

`k check` is what CI runs: it type-checks the whole project, checks the lockfiles and then runs every
test. `k test` runs only the tests, and `k test --watch` re-runs the affected ones on every save.

## Build it

```console
$ k build
wrote routes.lock
built dist
```

`k build` compiles the production build into `dist/`. `dist/app` is one executable file holding the
server, the JavaScript runtime and every file the browser loads, so running the app anywhere means
running that one file:

```bash
K_PUBLIC_ORIGIN=http://localhost:8080 dist/app
```

`K_PUBLIC_ORIGIN` is the address people reach the app at. `routes.lock` records the URL of every page, and
`k check` compares the pages with it, so a link someone saved keeps working; commit it with the code.

## Recap

You've built a tip splitter, and on the way:

- created a project with `k new` and ran it with `k dev`;
- kept values in `@State`, computed others with `@Derived`, and bound fields to state with `bind`;
- built a component with `@Prop` inputs and an `@Emits` event, and listened to it with `.on(…)`;
- styled a page with a `style` block and theme tokens, and saw a misspelt class fail to compile;
- tested the page with `render` and `screen`, ran `k check`, and built `dist/app`.

## Next steps

- [Tutorial: build Shelf](tutorial/index.md) builds a larger app, one step at a time.
- [A tour of K](../tour.md) builds a small full-stack feature, from the database to the button.
- [Components](../apps/components.md), [Reactivity](../apps/reactivity.md) and
  [Styles and themes](../apps/styles.md) cover what this page used in full.
