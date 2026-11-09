import books.Book
import books.BookDraft
import books.BookReview
import books.BookService
import books.LookupError
import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// A stand-in for `BookService` that can't reach the server until
/// `reconnect()`, as if the browser were offline. This module is
/// test-only: only tests can use it.
public service OfflineBookService implements BookService {
    private var isOnline: Bool = false

    /// Makes later calls succeed, with an empty shelf.
    func reconnect() {
        isOnline = true
    }

    func all() async throws RpcOnlyError -> List<Book> {
        guard isOnline else {
            throw RpcOnlyError.rpcFailure(.offline)
        }

        return []
    }

    func get(id: Uuid) async throws LookupError -> Book {
        throw LookupError.rpcFailure(.offline)
    }

    func add(draft: BookDraft) async throws RpcOnlyError -> Book {
        throw RpcOnlyError.rpcFailure(.offline)
    }

    func review(book: Book, review: BookReview) async throws LookupError -> Book {
        throw LookupError.rpcFailure(.offline)
    }
}
