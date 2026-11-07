/// Requires a `String` field to hold an email address. Redundant (a
/// warning) on a [`k.text.EmailAddress`] field.
@Annotation
public struct Email {
    /// The message; the stdlib's default when `nil`.
    let message: String? = nil
}
