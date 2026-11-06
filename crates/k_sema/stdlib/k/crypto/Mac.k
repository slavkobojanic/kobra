/// A message authentication code. It doesn't implement `Equatable`, so the
/// constant-time `matches` is the only comparison that compiles.
@Native
public struct Mac {
    /// Compares with `other` in constant time.
    func matches(_ other: Bytes) -> Bool
    /// The digest.
    var bytes: Bytes
    /// The digest as lowercase hex.
    var hex: String
}
