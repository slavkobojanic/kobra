/// Mounts `view` in the test's browser container and returns a `Screen`
/// (`docs/design/21-testing.md`, "Rendering and querying"). A page renders
/// without its layout, with its guards applied.
@Native
public func render(_ view: View) -> Screen
