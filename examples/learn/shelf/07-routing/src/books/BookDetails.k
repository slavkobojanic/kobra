import k.crypto.Uuid

/// One book: its title and author, and the reader's notes and rating.
public page BookDetails at "/books/{id}" {
    /// The book to show, from the URL.
    @Param let id: Uuid

    @Inject let shelf: BookService

    @Load let book: Book = try await shelf.get(id: id)

    metadata {
        title("{book.title} · Shelf")
    }

    view {
        main {
            link(to: .home) { "All books" }

            heading(level: 1) { book.title }

            paragraph { "by {book.author}" }
                .class(.author)

            ReviewForm(book: book)
        }
        .class(.page)
    }

    loading {
        main {
            paragraph { "Loading the book…" }
        }
        .class(.page)
    }

    failed(let error) {
        main {
            match error {
                case .notFound {
                    metadata {
                        title("Book not found · Shelf")
                        status(.notFound)
                    }

                    paragraph { "This book isn't on your shelf." }
                }

                case .rpcFailure {
                    paragraph { "Shelf couldn't load this book." }

                    button { "Try again" }
                        .on(click: reload)
                }
            }

            link(to: .home) { "All books" }
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

        .author { color: .muted; }
    }
}
