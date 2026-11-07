/// Where pages render, set with [`Rendering`] on the `app`
/// (`docs/design/23-app-structure.md`, "Rendering mode").
public enum RenderingMode {
    /// On the server, then hydrated in the browser. The default.
    case server
    /// In the browser only.
    case client
}
