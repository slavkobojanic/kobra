// Constraint annotations apply to config fields; on a `Secret<T>` they check the revealed value.
import k.validation.Matches
import k.validation.NotBlank
import k.validation.Range

config StripeConfig {
    @Matches("^whsec_[A-Za-z0-9]+$") @Env("STRIPE_WEBHOOK_SECRET") let signingSecret: Secret<String>
    @Range(1...65535) @Env("STRIPE_PORT") let port: Int = 443
    @NotBlank @Env("STRIPE_ACCOUNT") let account: String
}
