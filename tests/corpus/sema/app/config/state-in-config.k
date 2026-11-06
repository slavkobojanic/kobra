// A config holds no state.
config MailConfig {
    @Env("SMTP_HOST") let host: String
    @State var sent: Int = 0
}
