import k.server.Email
import k.server.EmailError
import k.server.EmailProvider

/// An `EmailProvider` that keeps messages in memory instead of sending them.
///
/// ```k
/// test OrderConfirmationJob {
///     use RecordingMailer for EmailProvider
///
///     @Inject let mailer: RecordingMailer
/// }
/// ```
///
/// This module is test-only (see `docs/design/proposals/testing-support.md`).
public service RecordingMailer implements EmailProvider {
    private var failure: EmailError? = nil

    /// Every message sent so far, oldest first.
    public var sent: List<Email> = []

    /// Makes every later `send(email:)` fail, as an SMTP outage or a
    /// refusing server would.
    ///
    /// - Parameter error: the error to throw.
    public func simulateFailure(error: EmailError) {
        failure = error
    }

    /// Records the message, or throws the simulated failure.
    public func send(email: Email) async throws EmailError {
        if let failure {
            throw failure
        }

        sent.append(email)
    }
}
