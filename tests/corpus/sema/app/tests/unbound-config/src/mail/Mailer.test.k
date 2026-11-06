// Tests never read the environment: a group whose graph reaches an app config binds it.
test Unbound {
    @Inject let mailer: Mailer

    test "runs" {
        expect true
    }
}

test Bound {
    use MailConfig(host: "localhost") for MailConfig

    @Inject let mailer: Mailer

    test "runs" {
        expect true
    }
}
