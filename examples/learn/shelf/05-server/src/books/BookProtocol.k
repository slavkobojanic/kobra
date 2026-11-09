import k.rpc.RpcOnlyError

/// What the browser can ask the server to do with books.
public protocol BookProtocol {
    /// Saves a book on the shelf. The draft's rules are checked again on the
    /// server before this runs.
    ///
    /// - Returns: the saved book, with the id the server gave it.
    rpc add(draft: BookDraft) throws RpcOnlyError -> Book
}
