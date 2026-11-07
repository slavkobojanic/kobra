/// The secret a serving app verifies service tokens with, for rpcs guarded
/// by `ServiceAuthenticated` (`docs/design/40-workspaces-and-services.md`).
/// It is the `token` of the callers' `RemoteService` configs. `k dev` at a
/// workspace root sets it, and the callers' tokens, to one generated
/// development secret.
public config ServiceConfig {
    /// The shared secret.
    @Env("K_SERVICE_TOKEN") let token: Secret<String>
}
