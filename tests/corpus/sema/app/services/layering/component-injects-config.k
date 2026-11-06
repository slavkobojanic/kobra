// A component can't inject a config.
config StripeConfig {
    @Env("STRIPE_KEY") let key: Secret<String>
}

component Pay {
    @Inject let config: StripeConfig

    view {
        text { "pay" }
    }
}
