import k.rpc.RemoteService

/// Where paybox runs, for the `remote` rule of `pay.Payments` in `App.k`.
///
/// `k dev` at the workspace root sets both values itself, to the paybox it
/// starts and a development secret generated for the run.
public config PayboxService implements RemoteService {
    /// Paybox's base URL, as the shop's server reaches it.
    @Env("PAYBOX_URL") let url: Url

    /// The secret the shop signs its calls to paybox with: paybox's own
    /// `K_SERVICE_TOKEN`.
    @Env("PAYBOX_TOKEN") let token: Secret<String>

    /// How long a call to paybox may take before checkout treats paybox as
    /// unavailable.
    @Env("PAYBOX_TIMEOUT") let timeout: Duration = 10.seconds
}
