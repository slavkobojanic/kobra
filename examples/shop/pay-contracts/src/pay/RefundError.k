import k.rpc.RpcFailure

/// Why a refund failed.
public enum RefundError implements Error, Codable {
    /// The calling merchant has no charge with this id.
    case unknownCharge

    /// The amount isn't positive, or isn't in the charge's currency.
    case invalidAmount

    /// The amount is more than what is left to refund, `refundable`.
    case exceedsCharge(refundable: Money)

    /// The call failed outside the handler: paybox refused the service
    /// token, was unreachable or didn't answer in time.
    case rpcFailure(RpcFailure)
}
