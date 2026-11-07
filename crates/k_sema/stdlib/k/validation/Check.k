/// An asynchronous rule run by a service:
/// `@Check(UserService.emailAvailable)`. Debounced on blur in the browser;
/// always run in full on the server.
@Annotation
public struct Check {
    /// Runs the service function `check` on the field's value.
    ///
    /// - Parameters:
    ///   - check: the checking function, `(T) async -> Bool`, written as
    ///     `Service.function`; `false` is a failure.
    ///   - message: the message shown on failure; the stdlib's default
    ///     when `nil`.
    @Native
    init(_ check: DeclarationName, message: String? = nil) {}
}
