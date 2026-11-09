import k.auth.Authenticated
import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// What the browser can ask the server to do with the books of the person
/// signed in. Only someone signed in may ask.
public protocol BookProtocol requires Authenticated {
    /// Returns every book on the shelf, the first one added first.
    rpc list() throws RpcOnlyError -> List<Book>

    /// Returns one book.
    ///
    /// - Throws: `LookupError.notFound` when there is no book with this id.
    rpc get(id: Uuid) throws LookupError -> Book

    /// Saves a book on the shelf. The draft's rules are checked again on the
    /// server before this runs.
    ///
    /// - Returns: the saved book, with the id the server gave it.
    rpc add(draft: BookDraft) throws RpcOnlyError -> Book

    /// Replaces a book's notes and rating.
    ///
    /// - Returns: the saved book.
    /// - Throws: `LookupError.notFound` when there is no book with this id.
    rpc review(id: Uuid, review: BookReview) throws LookupError -> Book
}
