import k.rpc.RpcFailure

/// Why a call about one existing note failed.
public enum LookupError implements Error {
    /// There is no note with this id, or it belongs to another account. The
    /// two look identical on purpose, so ids can't be probed.
    case notFound

    /// The call failed before `NotesApi` saw it: the `Authenticated` guard
    /// refused it because nobody is signed in.
    case rpcFailure(RpcFailure)
}
