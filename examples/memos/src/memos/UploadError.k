import k.rpc.RpcFailure

/// Why a memo wasn't stored.
public enum UploadError implements Error {
    /// The recording is empty, longer than three minutes, or not audio.
    case notAMemo
    /// The browser couldn't reach the server, or the server refused.
    case rpcFailure(RpcFailure)
}
