/// Where the third-party API is.
public config UpstreamConfig {
    /// The API's base URL; the harness points it at its fake.
    @Env("UPSTREAM_URL") let url: Url

    /// The API key, sent as a bearer token.
    @Env("UPSTREAM_TOKEN") let token: Secret<String>

    /// A base URL nothing answers at.
    @Env("OFFLINE_URL") let offlineUrl: Url
}
