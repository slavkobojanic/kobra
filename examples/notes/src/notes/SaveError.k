import k.rpc.RpcFailure

/// Why saving an edit to a note failed.
public enum SaveError implements Error {
    /// There is no note with this id, or it belongs to another account. It
    /// may have been deleted in another tab.
    case notFound

    /// The note was saved elsewhere, for example in another tab, after this
    /// edit started. `latest` is that newer version, so the person can decide
    /// whose changes to keep.
    case conflict(latest: NoteSummary)

    /// The call failed outside `NotesApi`: the `Authenticated` guard refused
    /// it because nobody is signed in, the draft failed validation on the
    /// server, or the browser couldn't reach the server.
    case rpcFailure(RpcFailure)
}
