/// How the app connects to its database.
public config DatabaseConfig {
    /// The database URL: `postgres://user@host:5432/db` or `sqlite:path`.
    @Env("DATABASE_URL") let url: Url
    /// The password, kept out of the URL.
    @Env("DATABASE_PASSWORD") let password: Secret<String>? = nil
}
