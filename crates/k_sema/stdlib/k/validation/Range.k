/// Bounds a number field: `@Range(13...120)`.
@Annotation
public struct Range {
    /// Requires the value to be in `bounds`.
    ///
    /// - Parameters:
    ///   - bounds: the allowed values, both ends included.
    ///   - message: the message shown on failure; the stdlib's default
    ///     when `nil`.
    @Native
    init(_ bounds: ClosedRange<Int>, message: String? = nil) {}
}
