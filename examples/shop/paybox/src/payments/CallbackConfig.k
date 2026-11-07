/// How paybox calls its merchants back.
///
/// Each merchant verifies paybox's calls with its own `K_SERVICE_TOKEN`, so
/// this token must be that secret. Without one, paybox signs with its own
/// `K_SERVICE_TOKEN`, which is what `k dev` at the workspace root gives
/// every app; a deployment gives the two services different secrets.
public config CallbackConfig {
    /// The secret paybox signs its calls to merchants with.
    @Env("PAYBOX_CALLBACK_TOKEN") let token: Secret<String>? = nil

    /// How long one delivery may take before it counts as failed and is
    /// retried.
    @Env("PAYBOX_CALLBACK_TIMEOUT") let timeout: Duration = 5.seconds
}
