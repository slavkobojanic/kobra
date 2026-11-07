import k.rpc.RpcFailure

/// Why checkout failed, as the browser sees it.
public enum CheckoutError implements Error, Codable {
    /// Paybox declined the amount.
    case declined
    /// Paybox couldn't be reached.
    case paymentsUnavailable
    /// The call from the browser failed.
    case rpcFailure(RpcFailure)
}
