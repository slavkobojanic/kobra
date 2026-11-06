// A service that injects itself is a cycle of one.
service Loop {
    @Inject let again: Loop
}
