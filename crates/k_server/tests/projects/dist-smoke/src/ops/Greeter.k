/// Server-only: reads the config.
public service Greeter {
    @Inject let config: SmokeConfig

    func greeting() -> String {
        return "{config.greeting} (token {config.token})"
    }
}
