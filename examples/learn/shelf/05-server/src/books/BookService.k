import k.rpc.RpcOnlyError

/// The browser's way to the books on the server. Components use this
/// service; only a service can call a protocol.
public service BookService {
    @Inject let api: BookProtocol

    /// Saves a book on the server.
    ///
    /// - Returns: the saved book.
    /// - Throws: `RpcOnlyError.rpcFailure` when the server can't be reached.
    func add(draft: BookDraft) async throws RpcOnlyError -> Book {
        return try await api.add(draft: draft)
    }
}
