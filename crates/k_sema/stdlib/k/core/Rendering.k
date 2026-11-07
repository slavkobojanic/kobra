/// Sets where pages render, on the `app`: `@Rendering(.client)`
/// (`docs/design/23-app-structure.md`).
@Annotation
public struct Rendering {
    /// Renders the app's pages as `mode` says ([`RenderingMode`]).
    @Native
    init(_ mode: RenderingMode) {}
}
