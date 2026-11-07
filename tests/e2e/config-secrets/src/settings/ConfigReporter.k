import k.observe.Log

/// A server-only service that reads the config.
public service ConfigReporter {
    @Inject let config: AppConfig

    /// The config as text. Secrets interpolate as `Secret(•••)`.
    func describe() -> String {
        let mode = if config.mode == .strict { "strict" } else { "relaxed" }
        let nickname = config.nickname ?? "nil"

        Log.info("config token is {config.token}")

        return "greeting={config.greeting} retries={config.retries} timeout={config.timeout.inSeconds}s mode={mode} tags={config.tags.count}:{config.tags.joined(separator: "|")} nickname={nickname} token={config.token} key={config.signingKey}"
    }

    /// Compares a candidate with the API token in constant time.
    func tokenMatches(candidate: String) -> Bool {
        return config.token.matches(candidate)
    }

    /// Compares a candidate with the signing key read from a file.
    func keyMatches(candidate: String) -> Bool {
        return config.signingKey.matches(candidate)
    }
}
