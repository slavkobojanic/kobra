// Config field types implement `ConfigValue`.
struct SmtpServer {
    let host: String
}

config MailConfig {
    @Env("SMTP_SERVER") let server: SmtpServer
    @Env("SMTP_HEADERS") let headers: Map<String, String>
    @Env("SMTP_HOST") let host: String
}
