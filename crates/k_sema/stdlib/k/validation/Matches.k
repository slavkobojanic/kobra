/// A `String` (or `Secret<String>`) field that matches a regular expression:
/// `@Matches("^[a-z0-9-]+$")`. The pattern must be a string literal; an
/// invalid pattern is a compile error.
@Annotation
public struct Matches {
    /// Requires the value to match `pattern`, a string literal.
    @Native
    init(_ pattern: String, message: String? = nil) {}
}
