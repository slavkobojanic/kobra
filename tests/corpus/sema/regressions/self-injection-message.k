// A service injecting itself is K4003; the message must not read "`Self` and
// `Self` depend on each other".
service Loop {
    @Inject let me: Loop
}
