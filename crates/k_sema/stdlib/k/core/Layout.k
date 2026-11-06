/// Wraps a view page, or every view page of the app, in a layout:
/// `@Layout(Dashboard)` (`docs/design/12-routing.md`).
@Annotation
public struct Layout {
    /// Names the layout by its type name.
    @Native
    init(_ layout: DeclarationName) {}
}
