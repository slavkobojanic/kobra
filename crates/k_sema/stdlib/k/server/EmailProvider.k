/// Sends email. A server-only platform interface; `SmtpEmailProvider` is
/// injected with no `use` rule, and `RecordingEmailProvider` replaces it in
/// tests.
@Platform(side: .server)
public interface EmailProvider {
    /// Sends `email`.
    func send(email: Email) async throws EmailError
}
