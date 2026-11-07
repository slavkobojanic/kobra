/// Every kind of config field: required and defaulted, secret and plain.
public config AppConfig {
    @Env("APP_GREETING") let greeting: String = "hello"
    @Env("APP_API_TOKEN") let token: Secret<String>
    @Env("APP_SIGNING_KEY") let signingKey: Secret<String>
    @Env("APP_RETRIES") let retries: Int = 3
    @Env("APP_TIMEOUT") let timeout: Duration = 90.seconds
    @Env("APP_MODE") let mode: Mode = .relaxed
    @Env("APP_TAGS") let tags: List<String> = []
    @Env("APP_NICKNAME") let nickname: String? = nil
}
