// A config holds `@Env` fields of `ConfigValue` types; it is server-only and injected into
// server-side services.
import k.text.EmailAddress
import k.validation.Range

enum Security {
    case none
    case startTls
}

config MailConfig {
//     ^^^^^^^^^^ side: server-only
    @Env("SMTP_HOST") let host: String
    @Range(1...65535) @Env("SMTP_PORT") let port: Int = 587
    @Env("SMTP_SECURITY") let security: Security = .startTls
    @Env("SMTP_PASSWORD") let password: Secret<String>
    @Env("SMTP_TIMEOUT") let timeout: Duration = 30.seconds
    @Env("MAIL_FROM") let from: EmailAddress
    @Env("MAIL_BCC") let bcc: List<String> = []
    @Env("MAIL_TRACKING") let tracking: Url? = nil
}

service Mailer {
//      ^^^^^^ side: server-only
    @Inject let config: MailConfig

    func describe() -> String {
        return "{config.host}:{config.port} {config.password}"
        //                                   ^^^^^^^^^^^^^^^ type: Secret<String>
    }
}
