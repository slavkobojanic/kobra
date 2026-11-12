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
