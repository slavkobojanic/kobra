/// Sets the lifetime of a service: `@Scope(.server)`
/// (`docs/design/10-services.md`, "Scopes").
@Annotation
public struct Scope {
    /// Gives the service the lifetime `scope` ([`ServiceScope`]).
    @Native
    init(_ scope: ServiceScope) {}
}
