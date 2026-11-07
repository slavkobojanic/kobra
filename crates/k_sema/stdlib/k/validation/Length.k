/// Bounds the length of a `String` field: `@Length(min: 8)`.
@Annotation
public struct Length {
    /// The minimum length, in characters; none when `nil`.
    let min: Int? = nil
    /// The maximum length, in characters; none when `nil`.
    let max: Int? = nil
    /// The message; the stdlib's default when `nil`.
    let message: String? = nil
}
