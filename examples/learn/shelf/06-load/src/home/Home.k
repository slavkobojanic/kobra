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

    metadata {
        title("Shelf")
    }

    view {
        main {
            heading(level: 1) { "Shelf" }

            paragraph { "The books you want to read, and the ones you've read." }

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
