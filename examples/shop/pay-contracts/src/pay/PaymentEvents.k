import k.rpc.RpcOnlyError
import k.rpc.ServiceAuthenticated

/// What paybox tells a merchant about its payments. Served by each merchant
/// (the shop) at the callback URL it registered with
/// `Payments.registerMerchant(callbackUrl:)`, and called by paybox.
///
/// Paybox delivers each event from a background job, retrying with backoff
/// while the merchant is unreachable or fails, so an event arrives at least
/// once and possibly more than once: implementations must be idempotent.
/// The two services call each other, so both depend on this library
/// instead of on each other.
public protocol PaymentEvents requires ServiceAuthenticated {
    /// `charge` succeeded.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the call fails outside the
    ///   handler; paybox then delivers the event again later.
    rpc chargeSucceeded(charge: Charge) throws RpcOnlyError

    /// `refund` was made.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the call fails outside the
    ///   handler; paybox then delivers the event again later.
    rpc chargeRefunded(refund: Refund) throws RpcOnlyError
}
