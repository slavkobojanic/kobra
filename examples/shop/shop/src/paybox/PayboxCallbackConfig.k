/// Where paybox delivers the shop's payment events.
///
/// This is the shop's base URL as paybox reaches it, which in a container
/// network or a cluster is an internal address (`http://shop:8080`) rather
/// than `K_PUBLIC_ORIGIN`. Paybox signs those calls with the shop's
/// `K_SERVICE_TOKEN`.
public config PayboxCallbackConfig {
    /// The shop's base URL for paybox.
    @Env("PAYBOX_CALLBACK_URL") let url: Url
}
