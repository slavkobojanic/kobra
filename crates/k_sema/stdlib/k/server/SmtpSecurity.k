/// How [`SmtpEmailProvider`] secures its connection.
public enum SmtpSecurity {
    /// STARTTLS on a plain connection.
    case startTls
    /// TLS from the start.
    case tls
    /// No encryption; for local catchers only.
    case none
}
