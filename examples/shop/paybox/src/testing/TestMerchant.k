import merchants.CallingMerchant

/// A `CallingMerchant` for tests, which call `PaymentsApi` in-process,
/// without a service token: the caller is the merchant `shop` until a test
/// calls `act(as:)`.
///
/// This module is test-only (docs/design/16-modules.md#test-only-modules).
public service TestMerchant implements CallingMerchant {
    /// The merchant the next call comes from; `nil` for a call without a
    /// token.
    var current: String? = "shop"

    /// Makes the following calls come from `merchant`, or from no merchant
    /// with `nil`.
    func act(as merchant: String?) {
        current = merchant
    }

    func name() -> String? {
        return current
    }
}
