// The extension from docs/design/08-types.md.
extend Int {
    var retries: RetryPolicy { RetryPolicy(attempts: self) }
}
