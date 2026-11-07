/// The server's settings (`docs/design/25-config-and-secrets.md`, "Stdlib
/// configs").
public config ServerConfig {
    /// The port `dist/app serve` listens on.
    @Env("PORT") let port: Int = 8080
    /// The origin users reach the app at; required in production.
    @Env("K_PUBLIC_ORIGIN") let publicOrigin: Url
    /// How long shutdown waits for in-flight work.
    @Env("K_SHUTDOWN_GRACE") let shutdownGrace: Duration = 25.seconds
    /// The CIDR ranges of trusted proxies, whose forwarded headers give
    /// the client address.
    @Env("K_TRUSTED_PROXIES") let trustedProxies: List<String> = []
}
