/// Mounts `view` in the test's browser container
/// (`docs/design/21-testing.md`, "Rendering and querying"). A page renders
/// without its layout, with its guards applied.
///
/// - Parameter view: the component or page to render, such as
///   `Counter(start: 1)`.
/// - Returns: the [`Screen`] to query and act on.
@Native
public func render(_ view: View) -> Screen
