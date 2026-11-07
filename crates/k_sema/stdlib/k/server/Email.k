import k.text.EmailAddress

/// An email message, a `@Builder` struct (`docs/design/31-email.md`).
@Builder
public struct Email {
    /// The recipient.
    let to: EmailAddress
    /// The subject; a line break makes [`EmailProvider.send`] throw
    /// [`EmailError.invalidMessage`].
    let subject: String
    /// The plain-text part; every message has one.
    let text: String
    /// The HTML part, sent alongside [`text`]; none when `nil`.
    let html: String? = nil
    /// The sender; [`SmtpConfig.from`] (`MAIL_FROM`) when `nil`.
    let from: EmailAddress? = nil
    /// The address replies go to; none when `nil`.
    let replyTo: EmailAddress? = nil
    /// The addresses that receive a copy, visible to every recipient.
    let cc: List<EmailAddress> = []
    /// The addresses that receive a blind copy, hidden from the others.
    let bcc: List<EmailAddress> = []
}
