import k.rpc.RemoteService

/// Where paybox runs.
public config PayboxService implements RemoteService {
    @Env("PAYBOX_URL") let url: Url
    @Env("PAYBOX_TOKEN") let token: Secret<String>
}
