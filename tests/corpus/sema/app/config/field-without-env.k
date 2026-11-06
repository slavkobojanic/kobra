// Every config field names its variable with `@Env`.
config MailConfig {
    @Env("SMTP_HOST") let host: String
    let port: Int = 587
}
