// `Secret<T>` has no `==`, and neither do values computed from `reveal()`; `matches` compares in
// constant time.
config ApiConfig {
    @Env("API_TOKEN") let token: Secret<String>
}

service Auth {
    @Inject let config: ApiConfig

    func check(candidate: String) -> Bool {
        let direct = config.token == candidate
        let revealed = config.token.reveal() == candidate
        return config.token.matches(candidate)
    }
}
