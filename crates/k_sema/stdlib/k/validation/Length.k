/// The length of a `String` field: `@Length(min: 8)`.
@Annotation
public struct Length {
    /// The minimum length.
    let min: Int? = nil
    /// The maximum length.
    let max: Int? = nil
    /// The message; the stdlib's default when `nil`.
    let message: String? = nil
}
