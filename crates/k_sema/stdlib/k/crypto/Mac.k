/// A message authentication code, from [`Hmac`]. It doesn't implement
/// [`Equatable`], so the constant-time [`matches`] is the only comparison
/// that compiles.
@Native
public struct Mac {
    /// Compares with `other` in constant time.
    ///
    /// - Parameter other: the code received, such as a webhook signature.
    /// - Returns: `true` if `other` is exactly this code.
    func matches(_ other: Bytes) -> Bool
    /// The code's bytes.
    var bytes: Bytes
    /// The code as lowercase hex.
    var hex: String
}
