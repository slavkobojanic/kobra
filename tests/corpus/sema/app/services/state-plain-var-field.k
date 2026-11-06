// The rule covers plain `var` fields, not only `@State`.
service Retry {
    var attempts: Int = 0
}

component RetryButton {
    @Inject let retry: Retry

    func clear() {
        retry.attempts = 0
    }

    view {
        button { "Clear" }
            .on(click: clear)
    }
}
