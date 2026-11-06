/// The lifetime of a service: `@Scope(.server)`
/// (`docs/design/10-services.md`, "Scopes").
@Annotation
public struct Scope {
    /// Sets the scope.
    @Native
    init(_ scope: ServiceScope) {}
}
