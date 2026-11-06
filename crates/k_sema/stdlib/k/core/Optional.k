/// A value that may be absent, written `T?`. `nil` is the absent value.
///
/// Optionals are unwrapped with `if let`, `guard let`, `?.` and `??`; there
/// is no force unwrap. Optional types don't nest (`T??` is K0120).
public enum Optional<T> {
    /// No value; written `nil`.
    case none
    /// A value.
    case some(T)
}
