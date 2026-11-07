import k.text.EmailAddress

/// SMTP settings, required only when [`SmtpEmailProvider`] is reachable
/// (`docs/design/31-email.md`, "SMTP configuration").
public config SmtpConfig {
    /// The server's host name.
    @Env("SMTP_HOST") let host: String
    /// The server's port.
    @Env("SMTP_PORT") let port: Int = 587
    /// How the connection is secured ([`SmtpSecurity`]).
    @Env("SMTP_SECURITY") let security: SmtpSecurity = .startTls
    /// The user name; `nil` for no authentication.
    @Env("SMTP_USERNAME") let username: String? = nil
    /// The password, used with [`username`].
    @Env("SMTP_PASSWORD") let password: Secret<String>? = nil
    /// The sender of messages that set no [`Email.from`].
    @Env("MAIL_FROM") let from: EmailAddress
}
