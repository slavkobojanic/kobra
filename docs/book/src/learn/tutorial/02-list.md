# 2. Show a list of books

In this step you'll describe a book as a type of your own, keep a few books in a list, and show each one
with a component you write. By the end, the start page lists three books. The books are written into the
code for now; step 3 lets you add your own.

Start from the project you finished in [step 1](01-create.md), with `k dev` running. The finished step is
in the repository as `examples/learn/shelf/02-list`.

## Describe a book

Every book on the shelf has a title and an author. A struct is a type made of named fields, so a book is
a struct. Create the directory `src/books/`, a new module, and in it the file `src/books/Book.k`:

```k
// examples/learn/shelf/02-list/src/books/Book.k
/// A book on the reading list.
public struct Book {
    /// Tells the books apart, even two with the same title.
    let id: Int
    /// The book's title, as printed on the cover.
    let title: String
    /// Who wrote it.
    let author: String
}
```

- `public` lets other modules use `Book`. Without it, only the code in `src/books/` could.
- Each `let` line is a field: its name, a colon, and its type. `Int` is a whole number and `String` is
  text. A field declared with `let` never changes once the book is made.
- `id` is a number that's different for every book. Two books can share a title, and the list needs a way
  to tell them apart.

You make a book by calling the type with a value for every field, each named:
`Book(id: 1, title: "Middlemarch", author: "George Eliot")`. Leaving a field out is a compile error, so a
half-made book never exists.

## Show one book

A component is a piece of a view you can use in many places, the way you use a built-in element. One
component, `BookRow`, shows one book. Create `src/books/BookRow.k`:

```k
// examples/learn/shelf/02-list/src/books/BookRow.k
/// One book in the reading list: its title, and who wrote it.
public component BookRow {
    /// The book to show.
    @Prop let book: Book

    view {
        item {
            emphasis { book.title }

            text { " by {book.author}" }
        }
    }
}
```

- `@Prop let book: Book` declares a prop: an input the component is given by whoever uses it. The
  component reads it and never changes it. `@Prop` is an annotation, a word starting with `@` that says
  what kind of member the line declares.
- `item` is one entry of a list; it only goes inside a `list`. `emphasis` sets the title apart, in
  italics as book titles usually are, and `text` is a run of plain text.
- `{book.author}` inside a string puts the value in the text, so this reads "by George Eliot".
  `book.author` is the `author` field of the `book` prop.

`BookRow` uses `Book` without importing it, because both are in the module `books`.

## Show the list

Now the page needs some books and a list to show them in. Replace the contents of `src/home/Home.k`:

```k
// examples/learn/shelf/02-list/src/home/Home.k
import books.Book
import books.BookRow

/// The app's start page: the reading list.
public page Home at "/" {
    let books: List<Book> = [
        Book(id: 1, title: "The Left Hand of Darkness", author: "Ursula K. Le Guin"),
        Book(id: 2, title: "Middlemarch", author: "George Eliot"),
        Book(id: 3, title: "The Remains of the Day", author: "Kazuo Ishiguro")
    ]

    metadata {
        title("Shelf")
    }

    view {
        main {
            heading(level: 1) { "Shelf" }

            paragraph { "The books you want to read, and the ones you've read." }

            list {
                for book in books key book.id {
                    BookRow(book: book)
                }
            }
            .label("Books")
            .class(.books)
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

        .books {
            listStyle: none;
            padding: 0;
        }
    }
}
```

Save and reload the browser: the three books appear under the heading, each title in italics.

- `import books.Book` makes `Book` from the module `books` usable here, by its short name. The page is in
  the module `home`, so it imports what it uses from `books`.
- `let books: List<Book> = [ … ]` is a constant of the page: a `List` of `Book`s, written as the books
  between square brackets, separated by commas.
- `list` is a list on the page, a bulleted list in HTML. `.label("Books")` gives it a name that screen readers
  announce.
- `for book in books key book.id { … }` repeats its contents once for each book, with `book` standing for
  the current one. The `key` is what tells the books apart, so when the list changes later, each row stays
  with its book. Use something that's different for every item, such as an id.
- `BookRow(book: book)` uses your component, passing the current book as its `book` prop. A prop is
  passed like a named argument.
- The new `.books` class removes the list's bullets and its indent.

**Try it:** leave the prop out. Change `BookRow(book: book)` to `BookRow()`, save, and run `k check`:

```text
error[K2003]: missing argument(s) `book`
  --> src/home/Home.k:24:29
   |
24 |                     BookRow()
   |                             ^

checked 6 files: 1 error(s), 0 warning(s)
```

A prop declared with `@Prop let` and no default is required, so a row without its book never compiles.
Now pass the wrong kind of value instead, `BookRow(book: book.title)`:

```text
error[K2001]: expected `books.Book`, found `String`
  --> src/home/Home.k:24:35
   |
24 |                     BookRow(book: book.title)
   |                                   ^^^^^^^^^^

checked 6 files: 1 error(s), 0 warning(s)
```

The prop's type is `Book`, and a title is a `String`. Change it back to `BookRow(book: book)`.

## Check it

The test from step 1 still passes: the page still has its heading. Run `k check`:

```console
$ k check
checked 6 files, ran 1 test: no problems
```

Two more files are checked than in step 1, `Book.k` and `BookRow.k`. In the next step you'll write tests
for the books too.

## Recap

In this step you:

- described a book as a `struct` with typed fields, and made books by naming every field;
- kept the books in a `List`, a constant of the page;
- wrote a component, `BookRow`, whose input is a `@Prop`, and used it like a built-in element;
- repeated part of a view with `for … in … key …`, keyed by each book's id;
- imported a declaration from another module, and saw a missing or wrongly typed prop fail to compile.

## Next

- [3. Add books with a form](03-form.md): let people add their own books, with checks on what they type.
- [Components](../../apps/components.md) covers props and components in full, and
  [Structs, enums, classes and interfaces](../../language/declarations.md) covers structs.
