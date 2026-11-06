/// A `String` field that isn't empty after trimming.
@Annotation
public struct NotBlank {
    /// The message; the stdlib's default when `nil`.
    let message: String? = nil
}
