/// Builds a client of a protocol at an address known only at run time,
/// such as a merchant's callback URL registered with the app
/// (`docs/design/40-workspaces-and-services.md`). The protocol is the
/// type the result is used as:
/// `let events: PaymentEvents = RemoteClient.at(url: hook, token: secret)`.
/// Calls go through the same codecs, timeouts and error mapping as a
/// `remote` rule's client ([`RemoteService`]). Server-only.
@ServerOnly
public struct RemoteClient {
    /// Builds a client of protocol `P`, the type the result is used as.
    ///
    /// - Parameters:
    ///   - url: where the protocol is served.
    ///   - token: the secret the calls are signed with.
    ///   - timeout: how long to wait for each answer.
    /// - Returns: the client; failures outside the remote handler reach
    ///   the caller as `rpcFailure` cases ([`RpcFailure`]).
    @Native
    static func at<P>(url: Url, token: Secret<String>, timeout: Duration = 10.seconds) -> P
}
