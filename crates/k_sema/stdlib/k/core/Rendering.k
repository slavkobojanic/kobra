/// Sets where pages render, on the `app`: `@Rendering(.client)`
/// (`docs/design/23-app-structure.md`). On a component, `@Rendering(.client)`
/// renders only that component in the browser: the server renders its
/// `loading` block (`docs/design/47-client-rendered-components.md`).
@Annotation
public struct Rendering {
    /// Renders the app's pages, or the component, as `mode` says
    /// ([`RenderingMode`]).
    @Native
    init(_ mode: RenderingMode) {}
}
