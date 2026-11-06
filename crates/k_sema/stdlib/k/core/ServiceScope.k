/// The lifetime of a service instance, set with `@Scope`
/// (`docs/design/10-services.md`, "Scopes").
public enum ServiceScope {
    /// The default: one per request on the server, one per app in the
    /// browser.
    case app
    /// One per request; server-only.
    case request
    /// As long as the route is mounted.
    case route
    /// One per injecting subtree.
    case component
    /// One per server process; server-only.
    case server
}
