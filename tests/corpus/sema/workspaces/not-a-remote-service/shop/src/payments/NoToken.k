import k.rpc.RemoteService

/// A `RemoteService` without its token.
public config NoToken implements RemoteService {
    @Env("NO_TOKEN_URL") let url: Url
    @Env("NO_TOKEN_TIMEOUT") let timeout: Int = 3
}
