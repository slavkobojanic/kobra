/// Why sending an email ([`EmailProvider.send`]) failed, matching the
/// caller's retry decision.
public enum EmailError implements Error, Codable {
    /// The provider couldn't connect, timed out, or gave a temporary (4xx)
    /// SMTP reply; worth retrying.
    case unavailable
    /// A permanent (5xx) SMTP reply, with the server's text; not worth
    /// retrying.
    case rejected(reason: String)
    /// The message can't be sent as written (such as a line break in the
    /// subject).
    case invalidMessage(reason: String)
}
