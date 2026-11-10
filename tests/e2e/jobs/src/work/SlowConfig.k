/// Where the scheduled job waits.
public config SlowConfig {
    /// An API that answers after a second and a half: the harness points
    /// it at its fake (`.env`). Elsewhere nothing listens there, and the
    /// job doesn't mind.
    @Env("SLOW_URL") let url: Url = Url("http://127.0.0.1:9")
}
