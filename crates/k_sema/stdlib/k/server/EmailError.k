/// Why sending an email failed, matching the caller's retry decision.
public enum EmailError implements Error, Codable {
    /// Can't connect, timed out, or a temporary (4xx) SMTP reply.
    case unavailable
    /// A permanent (5xx) SMTP reply, with the server's text.
    case rejected(reason: String)
    /// The message can't be sent as written.
    case invalidMessage(reason: String)
}
