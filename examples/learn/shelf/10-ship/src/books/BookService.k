import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// The browser's way to the books on the server. Components use this
/// service; only a service can call a protocol.
public service BookService {
    @Inject let api: BookProtocol

    /// Loads every book on the shelf from the server.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached.
    func all() async throws RpcOnlyError -> List<Book> {
        return try await api.list()
    }

    /// Loads one book from the server.
    ///
    /// - Throws: `LookupError.notFound` when there is no such book, and
    ///   `.rpcFailure` when the server can't be reached.
    func get(id: Uuid) async throws LookupError -> Book {
        return try await api.get(id: id)
    }

    /// Saves a book on the server.
    ///
    /// - Returns: the saved book.
    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached.
    func add(draft: BookDraft) async throws RpcOnlyError -> Book {
        return try await api.add(draft: draft)
    }

    /// Saves a book's notes and rating on the server.
    ///
    /// - Returns: the saved book.
    /// - Throws: `LookupError.notFound` when the book was deleted, and
    ///   `.rpcFailure` when the server can't be reached.
    func review(book: Book, review: BookReview) async throws LookupError -> Book {
        return try await api.review(id: book.id, review: review)
    }
}
