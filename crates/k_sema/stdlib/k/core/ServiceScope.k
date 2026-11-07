/// The lifetime of a service instance, set with [`Scope`]
/// (`docs/design/10-services.md`, "Scopes").
public enum ServiceScope {
    /// One instance per request on the server and one per app in the
    /// browser; the default.
    case app
    /// One instance per request; server-only.
    case request
    /// One instance that lives as long as the route is mounted.
    case route
    /// One instance per injecting component subtree.
    case component
    /// One instance per server process; server-only.
    case server
}
