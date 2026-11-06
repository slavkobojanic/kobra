// path: src/corpus/Mail.test.k
// A test group binds a config to a value with `use <expression> for <Config>`.
config MailConfig {
    @Env("SMTP_HOST") let host: String
}

service Mailer {
    @Inject let config: MailConfig
}

test Mailer {
    use MailConfig(host: "localhost") for MailConfig
    //  ^^^^^^^^^^ resolves: decl corpus.MailConfig

    @Inject let mailer: Mailer

    test "has a host" {
        expect true
    }
}
