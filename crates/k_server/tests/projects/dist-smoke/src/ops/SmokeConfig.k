import k.validation.Range

/// Required and constrained values, so a missing or invalid environment
/// stops startup with exit 78.
public config SmokeConfig {
    @Env("SMOKE_GREETING") let greeting: String
    @Env("SMOKE_TOKEN") let token: Secret<String>
    @Range(1...10) @Env("SMOKE_WORKERS") let workers: Int = 2
}
