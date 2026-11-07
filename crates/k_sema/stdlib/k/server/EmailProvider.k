/// Sends email. A server-only platform interface; [`SmtpEmailProvider`] is
/// injected with no `use` rule, and [`k.test.RecordingEmailProvider`]
/// replaces it in tests.
@Platform(side: .server)
public interface EmailProvider {
    /// Sends `email`.
    ///
    /// - Throws: [`EmailError.unavailable`] for a temporary failure,
    ///   [`EmailError.rejected`] for a permanent one and
    ///   [`EmailError.invalidMessage`] for a message that can't be sent.
    func send(email: Email) async throws EmailError
}
