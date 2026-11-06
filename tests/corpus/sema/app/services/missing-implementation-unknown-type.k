// An `@Inject` field whose type doesn't resolve reports only K1202, not a missing implementation.
service Welcome {
    @Inject let mailer: Mailr
}
