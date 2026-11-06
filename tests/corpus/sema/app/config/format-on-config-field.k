// `@Format` is a form message; config fields report parse failures at startup instead.
import k.validation.Format

config MailConfig {
    @Format(message: "Enter a port") @Env("SMTP_PORT") let port: Int
}
