// `config` is contextual: only `config Name` at item or member start is a declaration.
service S {
    @Inject let config: StripeWebhookConfig

    func check() -> Bool {
        let tolerance = config.tolerance
        return config(tolerance)
    }
}
