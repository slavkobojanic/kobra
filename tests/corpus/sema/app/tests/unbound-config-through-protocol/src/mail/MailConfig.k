// An app config.
public config MailConfig {
    @Env("SMTP_HOST") let host: String
}
