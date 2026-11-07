/// The standard [`EmailProvider`], over SMTP with [`SmtpConfig`].
@ServerOnly
@Native
public service SmtpEmailProvider implements EmailProvider {
    /// Sends `email` over SMTP; see [`EmailProvider.send(email:)`].
    func send(email: Email) async throws EmailError
}
