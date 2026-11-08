import k.rpc.RpcFailure

/// Why a charge failed.
public enum ChargeError implements Error, Codable {
    /// The card was declined.
    case declined
    /// The call failed outside the handler.
    case rpcFailure(RpcFailure)
}
