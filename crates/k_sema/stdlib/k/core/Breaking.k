/// Allows a deliberate breaking change to something a lockfile records:
/// `@Breaking("reason")` (`docs/design/01-principles.md`, "Lockfiles").
@Annotation
public struct Breaking {
    /// States why the change is acceptable.
    @Native
    init(_ reason: String) {}
}
