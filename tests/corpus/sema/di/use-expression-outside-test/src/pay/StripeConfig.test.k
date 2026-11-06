test StripeConfig {
    use StripeConfig(key: "test") for StripeConfig

    test "has a key" {
        expect true
    }
}
