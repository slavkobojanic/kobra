import k.rpc.RpcFailure

/// Why a call about one book failed.
public enum LookupError implements Error, Codable {
    /// There is no book with this id.
    case notFound

    /// The call didn't reach `BookApi`: the browser couldn't reach the
    /// server, for example.
    case rpcFailure(RpcFailure)
}
