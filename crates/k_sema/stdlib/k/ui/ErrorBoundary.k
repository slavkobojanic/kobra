/// Catches panics in its content and renders `fallback` instead
/// (`docs/design/09-errors.md`, "Panics").
@Native
public component ErrorBoundary {
    /// The content.
    @Slot let content: View
    /// What renders after a panic.
    @Slot let fallback: View? = nil
}
