import k.text.EmailAddress

/// An email message, a `@Builder` struct (`docs/design/31-email.md`).
@Builder
public struct Email {
    /// The recipient.
    let to: EmailAddress
    /// The subject; a line break makes `send` throw `invalidMessage`.
    let subject: String
    /// The plain-text part; every message has one.
    let text: String
    /// The HTML part.
    let html: String? = nil
    /// The sender; defaults to `MAIL_FROM`.
    let from: EmailAddress? = nil
    /// The reply-to address.
    let replyTo: EmailAddress? = nil
    /// Copies.
    let cc: List<EmailAddress> = []
    /// Blind copies.
    let bcc: List<EmailAddress> = []
}
