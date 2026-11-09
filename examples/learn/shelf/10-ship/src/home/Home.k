import accounts.AuthService
import books.AddBookForm
import books.Book
import books.BookRow
import books.BookService
import k.auth.Authenticated
import k.ui.Router

/// The app's start page: the reading list of the person signed in.
public page Home requires Authenticated at "/" {
    @Inject let shelf: BookService
    @Inject let auth: AuthService

    @Load let books: List<Book> = try await shelf.all()

    func bookAdded(book: Book) {
        reload()
    }

    func signOut() async {
        do {
            try await auth.signOut()

            Router.navigate(to: .signIn)
        } catch .rpcFailure {
            // Still signed in: the server couldn't be reached.
        }
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

            button { "Sign out" }
                .class(.signOut)
                .on(click: signOut)
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

        .signOut { marginTop: .lg; }
    }
}
