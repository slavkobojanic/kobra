import k.rpc.RemoteService

/// Where paybox runs, for `Payments`.
public config PayboxService implements RemoteService {
    /// Paybox's base URL.
    @Env("PAYBOX_URL") let url: Url
    /// The secret paybox verifies shop's calls with.
    @Env("PAYBOX_TOKEN") let token: Secret<String>
    /// How long a call may take.
    @Env("PAYBOX_TIMEOUT") let timeout: Duration = 10.seconds
}
