import k.server.Email
import k.server.EmailError
import k.server.EmailProvider

/// The default `EmailProvider` in `k test`: it keeps sent messages.
@TestDefault(replaces: EmailProvider)
@Native
public service RecordingEmailProvider implements EmailProvider {
    /// Every message sent so far, oldest first.
    var sent: List<Email>

    /// Records `email`, or throws the simulated failure.
    func send(email: Email) async throws EmailError
    /// Makes every later `send` throw `error`.
    func simulateFailure(error: EmailError)
}
