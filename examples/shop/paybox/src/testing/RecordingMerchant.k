import k.rpc.RpcFailure
import pay.Charge
import pay.Refund
import payments.MerchantNotifier

/// A `MerchantNotifier` for tests: every merchant answers in-process, the
/// events it receives are recorded, and it can be told to fail like a
/// merchant that is unreachable.
public service RecordingMerchant implements MerchantNotifier {
    /// The succeeded charges received, oldest first.
    var charges: List<Charge> = []

    /// The refunds received, oldest first.
    var refunds: List<Refund> = []

    /// The callback URL of every call, oldest first.
    var urls: List<Url> = []

    /// The failure every call answers with, or `nil` to take the events.
    var failure: RpcFailure? = nil

    /// Makes every following call fail with `failure`, or succeed with `nil`.
    func fail(with failure: RpcFailure?) {
        self.failure = failure
    }

    func chargeSucceeded(_ charge: Charge, at callbackUrl: Url) async -> RpcFailure? {
        urls.append(callbackUrl)

        if let failure {
            return failure
        }

        charges.append(charge)

        return nil
    }

    func chargeRefunded(_ refund: Refund, at callbackUrl: Url) async -> RpcFailure? {
        urls.append(callbackUrl)

        if let failure {
            return failure
        }

        refunds.append(refund)

        return nil
    }
}
