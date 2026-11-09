// A test's in-process calls reach the protocol's server side, so the config it injects is bound.
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
