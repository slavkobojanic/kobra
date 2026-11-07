/// Paybox: a payment provider for the other services of the system, which
/// never moves real money. It serves `pay.Payments` to merchants and calls
/// each merchant's `pay.PaymentEvents` at the callback URL the merchant
/// registered.
///
/// Nothing needs choosing here. Paybox has no `remote` rule, because its
/// merchants aren't known when it is built: `payments.EventDeliveryJob`
/// builds a client for each one at run time with `RemoteClient.at`. Every
/// other service (`k.db.Database`, `k.server.JobQueue`, `k.time.Clock`) has
/// exactly one implementation outside tests, configured from the
/// environment as described in `README.md`.
public app Paybox {}
