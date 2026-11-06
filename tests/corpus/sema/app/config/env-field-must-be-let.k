// `@Env` fields are `let`.
config MailConfig {
    @Env("SMTP_HOST") var host: String
}
