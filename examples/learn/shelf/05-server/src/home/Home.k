import books.AddBookForm
import books.Book
import books.BookRow

/// The app's start page: the reading list.
public page Home at "/" {
    @State var books: List<Book> = []

    func add(book: Book) {
        books.append(book)
    }

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

            heading(level: 2) { "Add a book" }

            AddBookForm()
                .on(added: add)
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
            border: 1px solid .border;
            borderRadius: .md;
            backgroundColor: .surface;
        }
    }
}
