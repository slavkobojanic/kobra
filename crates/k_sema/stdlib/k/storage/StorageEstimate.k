/// How much storage this origin uses and may use, in bytes, as the
/// browser estimates it (`navigator.storage.estimate()`): rounded and
/// padded, for display rather than for deciding whether a write fits.
public struct StorageEstimate implements Equatable, Codable {
    /// The bytes the origin's storage uses now.
    let usage: Int
    /// The bytes it may use before writes throw `.quotaExceeded`.
    let quota: Int
}
