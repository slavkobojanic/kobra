/// Requires a `String` field not to be empty after trimming.
@Annotation
public struct NotBlank {
    /// The message; the stdlib's default when `nil`.
    let message: String? = nil
}
