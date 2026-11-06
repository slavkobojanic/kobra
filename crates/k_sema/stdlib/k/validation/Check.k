/// An asynchronous rule run by a service:
/// `@Check(UserService.emailAvailable)`. Debounced on blur in the browser;
/// always run in full on the server.
@Annotation
public struct Check {
    /// Names the checking function, `(T) async -> Bool`.
    @Native
    init(_ check: DeclarationName, message: String? = nil) {}
}
