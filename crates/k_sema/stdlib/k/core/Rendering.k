/// Where pages render, on the `app`: `@Rendering(.client)`
/// (`docs/design/23-app-structure.md`).
@Annotation
public struct Rendering {
    /// Sets the rendering mode.
    @Native
    init(_ mode: RenderingMode) {}
}
