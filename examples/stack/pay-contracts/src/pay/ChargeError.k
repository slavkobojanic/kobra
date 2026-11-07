import k.rpc.RpcFailure

/// Why a charge failed.
public enum ChargeError implements Error, Codable {
    /// The amount isn't positive.
    case declined
    /// The call failed outside the handler: paybox refused the token, was
    /// unreachable or didn't answer in time.
    case rpcFailure(RpcFailure)
}
