// `App.k` can't bind a config to a value, so no secret can be committed there.
import mail.MailConfig

public app Shop {
    use MailConfig(host: "localhost") for MailConfig

    metadata {
        title("Shop")
    }
}
