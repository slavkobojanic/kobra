// Outside test code, a config can't be constructed: production values come from the environment.
config MailConfig {
    @Env("SMTP_HOST") let host: String
}

service Mailer {
    func local() -> MailConfig {
        return MailConfig(host: "localhost")
    }
}
