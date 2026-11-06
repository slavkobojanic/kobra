import k.rpc.RpcFailure

/// Why a call about one existing note failed.
public enum LookupError implements Error {
    /// There is no note with this id, or it belongs to another account. The
    /// two look identical on purpose, so ids can't be probed.
    case notFound

    /// The call failed outside `NotesApi`: the `Authenticated` guard refused
    /// it because nobody is signed in, or the browser couldn't reach the
    /// server.
    case rpcFailure(RpcFailure)
}
