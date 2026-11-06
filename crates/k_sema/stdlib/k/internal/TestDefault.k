/// Marks a test double as the default implementation of `replaces` in
/// `k test` (`docs/design/21-testing.md`, "Test doubles and defaults").
///
/// A test reaches the double with `@Inject` by its concrete type, and a
/// `use` rule that restates the default is a warning.
@Annotation
public struct TestDefault {
    /// The interface or service the double replaces in tests.
    let replaces: DeclarationName
}
