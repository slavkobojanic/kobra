// A config injects nothing.
import k.time.Clock

config MailConfig {
    @Env("SMTP_HOST") let host: String
    @Inject let clock: Clock
}
