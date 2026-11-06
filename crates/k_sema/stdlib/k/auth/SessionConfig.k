/// Session lifetimes (`docs/design/25-config-and-secrets.md`, "Stdlib
/// configs").
public config SessionConfig {
    /// The absolute lifetime from sign-in.
    @Env("K_SESSION_MAX_AGE") let maxAge: Duration = 30.days
    /// The idle timeout, which slides on use.
    @Env("K_SESSION_IDLE") let idleTimeout: Duration = 7.days
}
