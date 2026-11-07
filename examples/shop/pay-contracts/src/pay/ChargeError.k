import k.rpc.RpcFailure

/// Why a charge failed.
public enum ChargeError implements Error, Codable {
    /// The caller hasn't called `Payments.registerMerchant(callbackUrl:)`
    /// yet, or the call didn't come with a service token.
    case unknownMerchant

    /// The amount isn't positive.
    case invalidAmount

    /// The card number isn't a card number: it has letters, the wrong
    /// number of digits, or fails the Luhn check.
    case invalidCard

    /// The card was declined, for `reason`. Another card may work.
    case declined(reason: DeclineReason)

    /// The call failed outside the handler: paybox refused the service
    /// token, was unreachable or didn't answer in time.
    case rpcFailure(RpcFailure)
}
