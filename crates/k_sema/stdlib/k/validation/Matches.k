/// Requires a `String` (or `Secret<String>`) field to match a regular
/// expression:
/// `@Matches("^[a-z0-9-]+$")`. The pattern must be a string literal; an
/// invalid pattern is a compile error.
@Annotation
public struct Matches {
    /// Requires the value to match `pattern`.
    ///
    /// - Parameters:
    ///   - pattern: a regular expression, as a string literal.
    ///   - message: the message shown on failure; the stdlib's default
    ///     when `nil`.
    @Native
    init(_ pattern: String, message: String? = nil) {}
}
