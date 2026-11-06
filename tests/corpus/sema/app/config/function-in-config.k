// A config is data: no functions.
config MailConfig {
    @Env("SMTP_HOST") let host: String

    func url() -> String {
        return "smtp://{host}"
    }
}
