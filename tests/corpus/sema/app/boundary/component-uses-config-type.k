// A config type can't appear in client code.
config StripeConfig {
    @Env("STRIPE_KEY") let key: Secret<String>
}

component Pay {
    func describe(config: StripeConfig) -> String {
        return "configured"
    }

    view {
        text { "pay" }
    }
}
