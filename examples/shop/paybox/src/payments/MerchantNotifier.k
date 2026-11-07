import k.rpc.RpcFailure
import pay.Charge
import pay.Refund

/// Calls a merchant's `pay.PaymentEvents`.
///
/// `RemoteNotifier` is the only implementation outside tests, and calls the
/// merchant over the network. Tests bind `testing.RecordingMerchant`, which
/// answers in-process, since a test never opens a socket.
public interface MerchantNotifier {
    /// Tells the merchant at `callbackUrl` that `charge` succeeded.
    ///
    /// - Returns: `nil` when the merchant took the event, otherwise why the
    ///   call failed.
    func chargeSucceeded(_ charge: Charge, at callbackUrl: Url) async -> RpcFailure?

    /// Tells the merchant at `callbackUrl` about `refund`.
    ///
    /// - Returns: `nil` when the merchant took the event, otherwise why the
    ///   call failed.
    func chargeRefunded(_ refund: Refund, at callbackUrl: Url) async -> RpcFailure?
}
