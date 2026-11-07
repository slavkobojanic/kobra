/// A config that is not a `RemoteService`.
public config PlainConfig {
    @Env("PLAIN_URL") let url: Url
}
