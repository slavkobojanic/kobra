/// Builds a client of a protocol at an address known only at run time,
/// such as a merchant's callback URL registered with the app
/// (`docs/design/40-workspaces-and-services.md`). The protocol is the
/// type the result is used as:
/// `let events: PaymentEvents = RemoteClient.at(url: hook, token: secret)`.
/// Calls go through the same codecs, timeouts and error mapping as a
/// `remote` rule's client. Server-only.
@ServerOnly
public struct RemoteClient {
    /// A client of protocol `P` at `url`, signing its calls with `token`,
    /// waiting at most `timeout` for each answer.
    @Native
    static func at<P>(url: Url, token: Secret<String>, timeout: Duration = 10.seconds) -> P
}
