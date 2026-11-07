/// A value that must not leak (`docs/design/25-config-and-secrets.md`).
///
/// `Secret` implements neither [`Codable`] nor [`Equatable`] nor
/// [`Hashable`], so it can't cross the wire or be compared with `==`.
/// `"{secret}"` interpolates as `Secret(•••)`.
@Native
@ServerOnly
public struct Secret<T> implements ConfigValue {
    /// Wraps `value`; used by the runtime and by tests.
    @Native
    init(_ value: T) {}

    /// Returns the value, at the one point it is used. Greppable on
    /// purpose.
    func reveal() -> T
    /// Compares the value with `candidate` in constant time.
    ///
    /// - Returns: `true` if they are equal.
    func matches(_ candidate: T) -> Bool
}
