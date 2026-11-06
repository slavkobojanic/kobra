/// The standard `EmailProvider`, over SMTP with `SmtpConfig`.
@ServerOnly
@Native
public service SmtpEmailProvider implements EmailProvider {
    /// Sends `email` over SMTP.
    func send(email: Email) async throws EmailError
}
