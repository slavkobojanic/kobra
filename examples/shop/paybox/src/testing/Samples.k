// Sample values shared by tests, as static members added by public
// extensions. This module is test-only, so the extensions are visible to
// every test file and to no production code (docs/design/21-testing.md).

import pay.CardDetails
import pay.ChargeRequest
import payments.CallbackConfig
import payments.TestCards

public extend ChargeRequest {
    /// A $48.00 charge for the shop's order `order-1`, to `card`.
    ///
    /// - Parameters:
    ///   - card: the card number; the card that always succeeds by default.
    ///   - reference: the merchant's reference.
    static func sample(card: String = TestCards.success, reference: String = "order-1") -> ChargeRequest {
        return ChargeRequest(reference: reference, amount: 48.00.usd, card: CardDetails(number: card), description: "2 × K Mug")
    }
}

public extend CallbackConfig {
    /// Signs callbacks with `cb_test_secret`, with the default timeout.
    static let sample: CallbackConfig = CallbackConfig(token: Secret("cb_test_secret"), timeout: 5.seconds)
}
