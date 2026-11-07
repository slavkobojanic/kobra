/// The service that made the current call of an rpc guarded by
/// [`ServiceAuthenticated`], as its verified service token names it
/// (`docs/design/40-workspaces-and-services.md`). Injected into the
/// service implementing the protocol; server-only.
@Scope(.app)
@ServerOnly
@Native
public service ServiceCaller {
    /// The calling app's `[package] name`; `nil` when the current request
    /// is not a verified service call.
    var name: String?
}
