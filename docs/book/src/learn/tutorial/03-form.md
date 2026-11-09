# 3. Add books with a form

In this step you'll add a form under the list, so you can put your own books on the shelf. The form
checks what you type before it adds anything, and shows a message under a field that needs fixing.
You'll also write your first tests.

Start from the project you finished in [step 2](02-list.md), with `k dev` running. The finished step is in
the repository as `examples/learn/shelf/03-form`.

## Say what a valid book is

A form needs to know two things: which fields it has, and what counts as a valid value for each. In K
both are written once, as a struct. Create `src/books/BookDraft.k`:

```k
// examples/learn/shelf/03-form/src/books/BookDraft.k
import k.validation.Length
import k.validation.NotBlank
import k.validation.Validatable

/// What the add-book form asks for, and the rules it must pass.
public struct BookDraft implements Validatable {
    @NotBlank(message: "Enter the book's title")
    @Length(max: 200)
    var title: String

    @NotBlank(message: "Enter who wrote it")
    @Length(max: 100)
    var author: String
}
```

- A draft is what someone types, before it becomes a `Book`, so it has no `id` yet.
- `implements Validatable` says the struct can be checked against rules. `Validatable` comes from
  `k.validation`, a module of K's standard library, imported like your own modules are.
- The annotations above each field are constraints: rules the field's value must pass.
  `@NotBlank` refuses text that is empty or only spaces, and its `message` is what the form shows when
  the rule fails. `@Length(max: 200)` refuses a title longer than 200 characters, with the standard
  library's own message.
- The fields are `var`, not `let`, because a form changes them as you type.

## Write the form

The form is a component of its own, `src/books/AddBookForm.k`:

```k
// examples/learn/shelf/03-form/src/books/AddBookForm.k
import k.validation.Form

/// A form for adding a book to the list. It checks what was typed,
/// reports a book that passes the checks with `added`, and then clears its
/// fields for the next one.
public component AddBookForm {
    @Form var draft: BookDraft = .empty

    /// Fired with the checked title and author when the form is submitted.
    @Emits let added: Event<BookDraft>

    func submit() {
        added.emit(draft.value)
        draft.reset()
    }

    view {
        form {
            textField(value: bind draft.title)
                .label("Title")

            textField(value: bind draft.author)
                .label("Author")

            button { "Add book" }
                .type(.submit)
        }
        .on(submit: draft.handle(submit))
    }
}
```

- `@Form var draft: BookDraft = .empty` keeps what's typed into each field of a `BookDraft`, and whether
  it passes the rules. `.empty` starts every field blank.
- `textField(value: bind draft.title)` is a text field for the title, and `.label("Title")` is the label
  shown next to it. `bind` lets the field change `draft.title` as you type. The form also connects the
  field to its error message, for the eye and for screen readers, without any more code.
- `.type(.submit)` makes the button submit the form, and so does pressing Enter in a field.
- `.on(submit: draft.handle(submit))` says what happens on submit. `draft.handle` checks every rule
  first, and shows each failure under its field. Only when they all pass does it call `submit`.
- `func submit()` declares a function of the component. `draft.value` is the checked `BookDraft`, and
  it can be read only inside the function passed to `handle`, where the rules are known to pass.
- `@Emits let added: Event<BookDraft>` declares an event: something that happened, which the component
  reports to whoever uses it. `added.emit(draft.value)` fires it with the draft. The form doesn't know
  about the list; it only says that a book was added.
- `draft.reset()` then puts every field back to how `.empty` started it, blank, and forgets the
  messages, so the form is ready for the next book.

## Keep the books in state

So far the page's books are a constant: a `let` never changes. To add to them, the page keeps them in
state. In `src/home/Home.k`, change `let books` to `@State var books` and add a function that adds a
book:

```k
// examples/learn/shelf/03-form/src/home/Home.k
import books.AddBookForm
import books.Book
import books.BookDraft
import books.BookRow

/// The app's start page: the reading list.
public page Home at "/" {
    @State var books: List<Book> = [
// ...
    ]

    func add(draft: BookDraft) {
        books.append(Book(id: books.count + 1, title: draft.title, author: draft.author))
    }
// ...
```

- `@State var` declares state: a value the page owns that can change. When it changes, the parts of the
  view that read it update, so the list shows the new book. You don't write any code to make that
  happen.
- `books.append(…)` adds a book to the end of the list. The new book's `id` is one more than the number
  of books, which is different from every id before it, because books are never removed yet.

Then put the form under the list, and listen to its event:

```k
// examples/learn/shelf/03-form/src/home/Home.k
// ...
            .label("Books")
            .class(.books)

            heading(level: 2) { "Add a book" }

            AddBookForm()
                .on(added: add)
        }
        .class(.page)
// ...
```

`.on(added: add)` calls `add` with the draft each time the form fires `added`. The page owns the books;
the form reports what was typed, and the page decides what to do with it.

Save and reload the browser. Type a title and an author and click Add book: the book joins the list,
and the fields clear for the next one. Now type only an author and click Add book again: "Enter the
book's title" appears under the field, and nothing is added.

The form is unstyled for now: the labels and fields sit on one line. Step 4 makes it look right.

**Try it:** read the draft outside `handle`. In `AddBookForm.k`, add the line
`paragraph { "Adding {draft.value.title}" }` to the form, between the Author field and the button, and
run `k check`:

```text
error[K7001]: `value` is read only inside the action passed to `handle(…)`, where every field parses
  --> src/books/AddBookForm.k:25:40
   |
25 |             paragraph { "Adding {draft.value.title}" }
   |                                        ^^^^^

checked 8 files: 1 error(s), 0 warning(s)
```

While someone is typing, the fields may not pass the rules, so there's no checked `BookDraft` to read.
The compiler only lets you read it where it's known to be valid. Remove the line.

**Try it:** misspell the event. Change `.on(added: add)` to `.on(add: add)` in the page:

```text
error[K3105]: `books.AddBookForm` has no event `add`: its events are `added`
  --> src/home/Home.k:39:21
   |
39 |                 .on(add: add)
   |                     ^^^

checked 8 files: 1 error(s), 0 warning(s)
```

Change it back.

## Check it

You've been checking the page by clicking. A test does the same clicking for you, every time you run
`k check`. Replace `src/home/Home.test.k` with three tests:

```k
// examples/learn/shelf/03-form/src/home/Home.test.k
import k.test.render

test Home {
    test "the start page shows the app's name" {
        let screen = render(Home())

        expect screen.heading(named: "Shelf").isVisible
    }

    test "a book added with the form joins the list" {
        let screen = render(Home())

        screen.textField(labeled: "Title").fill("Piranesi")
        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()

        expect screen.item(containing: "Piranesi").isVisible
        expect screen.textField(labeled: "Title").value == ""
    }

    test "a book needs a title" {
        let screen = render(Home())

        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()

        expect screen.text("Enter the book's title").isVisible
        expect screen.items().count == 3
    }
}
```

- `test Home { … }` groups the tests of `Home`, and each `test "…" { … }` is one test, named by what it
  shows.
- `render(Home())` shows the page as the browser would, and `screen` is what's on it.
- `screen` finds things the way a person using a screen reader does: a field by its label, a button by
  its name, a list item by its text. `fill` types into a field and `click` clicks.
- `expect` states what must be true. The second test also expects the Title field to be empty again,
  and the last test expects the error message and still three books.

Run `k check`:

```console
$ k check
checked 8 files, ran 3 tests: no problems
```

When an expectation fails, `k check` says which one, with the values it found. Change the `3` in the last
test to `4` and run it again to see:

```text
error[K7510]: expectation failed: screen.items().count == 4
  --> src/home/Home.test.k:28:16
   |
28 |         expect screen.items().count == 4
   |                ^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: `screen.items().count` was 3
   = note: in test `Home › a book needs a title`

checked 8 files, ran 3 tests: 1 error(s), 0 warning(s)
```

Change it back.

## Recap

In this step you:

- wrote the form's fields and rules once, as a `Validatable` struct with constraint annotations such as
  `@NotBlank` and `@Length`;
- built a form with `@Form`, bound its fields with `bind`, and let `handle` check the rules before
  `submit` runs;
- reported a new book from the form with an `@Emits` event, listened to it with `.on(added:)`, and
  cleared the form with `reset()`;
- kept the books in `@State`, so the list updates when one is added;
- tested the page with `render`, `screen` and `expect`, run by `k check`.

## Next

- [4. Make it look right](04-style.md): give Shelf its colors, and a dark mode.
- [Forms and validation](../../apps/forms.md), [Reactivity](../../apps/reactivity.md) and
  [Testing](../../apps/testing.md) cover forms, state and tests in full.
