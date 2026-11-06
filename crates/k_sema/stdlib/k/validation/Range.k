/// The range of a number field: `@Range(13...120)`.
@Annotation
public struct Range {
    /// Requires the value to be in `bounds`.
    @Native
    init(_ bounds: ClosedRange<Int>, message: String? = nil) {}
}
