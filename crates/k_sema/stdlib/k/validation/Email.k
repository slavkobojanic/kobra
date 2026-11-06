/// A `String` field that holds an email address. Redundant (a warning)
/// on an `EmailAddress` field.
@Annotation
public struct Email {
    /// The message; the stdlib's default when `nil`.
    let message: String? = nil
}
