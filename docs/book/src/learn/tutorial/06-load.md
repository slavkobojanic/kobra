# 6. Load books from the server

In this step you'll have the start page load the saved books from the server, so the shelf shows them
every time it opens. You'll say what the page shows while the books are on their way, and what it shows
when they can't be loaded, and refresh the list after each book you add.

Start from the project you finished in [step 5](05-server.md), with `k dev` running. The finished step is
in the repository as `examples/learn/shelf/06-load`.

## Ask the server for the books

The repository can already read every book, with `all()` from step 5. The browser needs an rpc to ask for
them. Add `list` to `src/books/BookProtocol.k`:

```k
// examples/learn/shelf/06-load/src/books/BookProtocol.k
// ...
public protocol BookProtocol {
    /// Returns every book on the shelf, the first one added first.
    rpc list() throws RpcOnlyError -> List<Book>
// ...
```

The compiler now reports that `BookApi` doesn't answer `list`. Answer it in `src/books/BookApi.k`:

```k
// examples/learn/shelf/06-load/src/books/BookApi.k
// ...
    rpc list() async throws RpcOnlyError -> List<Book> {
        let all = await records.all()

        return all.map { record => Book(id: record.id, title: record.title, author: record.author) }
    }
// ...
```

`map` makes a new list from an old one, by calling the lambda on each item: here, each stored record
becomes the `Book` the browser shows.

Then give the browser's service a function for it, in `src/books/BookService.k`:

```k
// examples/learn/shelf/06-load/src/books/BookService.k
// ...
    /// Loads every book on the shelf from the server.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached.
    func all() async throws RpcOnlyError -> List<Book> {
        return try await api.list()
    }
// ...
```

## Load them on the page

A page can't show the books until they've arrived, and they might not arrive at all. K handles both with a
`@Load` field: a value the page loads before it shows its view. Change the top of `src/home/Home.k`:

```k
// examples/learn/shelf/06-load/src/home/Home.k
import books.AddBookForm
import books.Book
import books.BookRow
import books.BookService

/// The app's start page: the reading list.
public page Home at "/" {
    @Inject let shelf: BookService

    @Load let books: List<Book> = try await shelf.all()

    func bookAdded(book: Book) {
        reload()
    }
// ...
```

- `@Load let books` runs `shelf.all()` when the page opens. The `view` is shown only once it has
  finished, so inside the view `books` is a plain list, and no code checks whether it's there yet.
- `bookAdded` calls `reload()`, which every page with a `@Load` field has: it loads the books again.
  While it runs, the view keeps showing the old list, and then shows the new one at once. The new book
  comes from the server, so what you see is what's saved.

Then, in the view, say something when the shelf is empty, and listen to the form with `bookAdded`:

```k
// examples/learn/shelf/06-load/src/home/Home.k
// ...
            if books.isEmpty {
                paragraph { "No books yet. Add the first one below." }
            } else {
                list {
                    for book in books key book.id {
                        BookRow(book: book)
                    }
                }
                .label("Books")
                .class(.books)
            }

            heading(level: 2) { "Add a book" }

            AddBookForm()
                .on(added: bookAdded)
        }
        .class(.page)
    }
// ...
```

`if … { … } else { … }` shows one part of the view or the other, and switches when `books` changes.

## Say what happens meanwhile

Next to `view`, a page with `@Load` fields has two more blocks. `loading` is shown until the books
arrive, and `failed` when loading them throws. Add both, after the `view` block:

```k
// examples/learn/shelf/06-load/src/home/Home.k
// ...
    loading {
        main {
            paragraph { "Loading your books…" }
        }
        .class(.page)
    }

    failed(let error) {
        main {
            match error {
                case .rpcFailure(.offline), .rpcFailure(.timeout) {
                    paragraph { "Shelf can't reach the server. Check your connection." }
                }

                case .rpcFailure {
                    paragraph { "Shelf couldn't load your books." }
                }
            }

            button { "Try again" }
                .on(click: reload)
        }
        .class(.page)
    }
// ...
```

- `failed(let error)` gets the error that `shelf.all()` threw. Its type is `RpcOnlyError`, the type the
  rpc declares, so the compiler knows every way it can fail.
- `match` picks the first `case` that fits the error, and shows its part of the view. Cases look inside
  the error: `.rpcFailure(.offline)` is a failed call because the browser is offline, and
  `.rpcFailure(.timeout)` one the server didn't answer in time. The last `case .rpcFailure` is every
  other failure.
- The button calls `reload()`, which tries again and shows `loading` while it does.

Save and reload the browser. The books you added in step 5 are back, and each book you add now stays after
a reload.

You'll rarely see the `loading` block here: when you open the page, the server loads the books before it
sends it, so it arrives complete, and `reload()` keeps the old list on screen. It shows when the page is
loaded in the browser, which step 7 does when you move between pages. The test below shows it now.

**Try it:** leave a failure out. Delete the last case, `case .rpcFailure { … }`, from the `match`:

```text
error[K2301]: the `match` doesn't handle every case
  --> src/home/Home.k:55:13
   |
55 |             match error {
   |             ^^^^^
   |
   = note: missing: `.rpcFailure(_)`
   = fix: Insert the missing arms

checked 14 files: 1 error(s), 0 warning(s)
```

The error's type lists every way it can fail, so a `match` that forgets one doesn't compile. A server that
later rejects a request (`.rpcFailure(.rejected(…))`, which step 8 brings) can't slip past this page
unnoticed. Put the case back.

**Try it:** forget that loading can fail. Remove `try` from the `@Load` line:

```text
error[K2201]: this call can throw and must be marked with `try`
  --> src/home/Home.k:10:35
   |
10 |     @Load let books: List<Book> = await shelf.all()
   |                                   ^^^^^^^^^^^^^^^^^
   |
   = fix: Insert `try ` before the expression

checked 14 files: 1 error(s), 0 warning(s)
```

Every call that can fail is marked, so you can see each one in the code. Put `try` back.

**Try it:** go offline while adding. In Chrome's DevTools, on the Network tab, set the throttling menu to
**Offline**, then add a book: "Couldn't save the book. Check your connection and try again." appears
above the button, from the `catch` you wrote in step 5, and the fields keep what you typed. Set it back to
**No throttling** and click Add book again.

## Check it

Run `k check` before you change the tests:

```text
error[K7510]: expectation failed: screen.heading(named: "Shelf").isVisible
  --> src/home/Home.test.k:10:16
   |
10 |         expect screen.heading(named: "Shelf").isVisible
   |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: in test `Home › the start page shows the app's name`

error[K7510]: textField(labeled: "Title") found nothing
  --> src/home/Home.test.k:13:10
   |
13 |     test "a book added with the form joins the list" {
   |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: in test `Home › a book added with the form joins the list`

error[K7510]: textField(labeled: "Title") found nothing
  --> src/home/Home.test.k:25:10
   |
25 |     test "a book added with the form is saved on the server" {
   |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: in test `Home › a book added with the form is saved on the server`

error[K7510]: textField(labeled: "Author") found nothing
  --> src/home/Home.test.k:39:10
   |
39 |     test "a book needs a title" {
   |          ^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: in test `Home › a book needs a title`

checked 14 files, ran 4 tests: 4 error(s), 0 warning(s)
```

In a test, `render` starts the page the way the browser does, so it shows `loading` first: there's no
heading and no form yet. Each test now waits for the books with `await screen.settle()` before it looks
at the page. Two new tests check the loading message, and that saved books are listed. Replace
`src/home/Home.test.k`:

```k
// examples/learn/shelf/06-load/src/home/Home.test.k
import books.BookDraft
import books.BookRepository
import books.BookService
import k.test.render

test Home {
    @Inject let records: BookRepository
    @Inject let shelf: BookService

    test "the start page shows the app's name" {
        let screen = render(Home())
        await screen.settle()

        expect screen.heading(named: "Shelf").isVisible
    }

    test "the page says it's loading until the books arrive" {
        let screen = render(Home())

        expect screen.text("Loading your books…").isVisible

        await screen.settle()

        expect screen.text("No books yet. Add the first one below.").isVisible
    }

    test "the saved books are listed when the page opens" {
        try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))
        try await shelf.add(draft: BookDraft(title: "Middlemarch", author: "George Eliot"))

        let screen = render(Home())
        await screen.settle()

        expect screen.items().count == 2
        expect screen.item(containing: "Piranesi").isVisible
    }

    test "a book added with the form joins the list" {
        let screen = render(Home())
        await screen.settle()
// ...
    }

    test "a book added with the form is saved on the server" {
        let screen = render(Home())
        await screen.settle()
// ...
    }

    test "a book needs a title" {
        let screen = render(Home())
        await screen.settle()
// ...
    }
}
```

- `@Inject let shelf: BookService` gives the test the browser's service, the one the page uses. The test
  adds two books through it, as the form would, before it renders the page.
- The first expectation of the loading test runs before `settle()`, while the books are on their way.

```console
$ k check
checked 14 files, ran 6 tests: no problems
```

## Recap

In this step you:

- loaded the books with a `@Load` field, so the `view` only ever sees a list that has arrived;
- said what the page shows meanwhile with `loading`, and on failure with `failed`;
- told failures apart with `match` on the error's cases, which must cover every one;
- refreshed the list after adding a book with `reload()`;
- waited for loads in tests with `await screen.settle()`.

## Next

- [7. A page per book](07-routing.md): a page for each book, with notes and a rating.
- [Async data loading](../../apps/async-data.md) covers `@Load`, `loading`, `failed` and `reload()` in
  full, and [Errors](../../language/errors.md) covers errors and `match`.
