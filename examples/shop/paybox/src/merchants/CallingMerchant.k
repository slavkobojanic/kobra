/// Names the merchant making the current call.
///
/// `VerifiedCaller` is the only implementation outside tests: the caller
/// name of the verified service token. Tests bind a fixed merchant instead,
/// since they call `PaymentsApi` in-process, without a token.
public interface CallingMerchant {
    /// The calling merchant's name, or `nil` when the current call didn't
    /// come with a verified service token.
    func name() -> String?
}
