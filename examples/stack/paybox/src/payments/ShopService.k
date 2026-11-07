import k.rpc.RemoteService

/// Where shop runs, for `PaymentEvents`.
public config ShopService implements RemoteService {
    /// Shop's base URL.
    @Env("SHOP_URL") let url: Url
    /// The secret shop verifies paybox's calls with.
    @Env("SHOP_TOKEN") let token: Secret<String>
    /// How long a call may take.
    @Env("SHOP_TIMEOUT") let timeout: Duration = 5.seconds
}
