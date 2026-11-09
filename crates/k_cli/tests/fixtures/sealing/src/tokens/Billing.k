import k.crypto.Jwt
import k.crypto.JwtError
import k.time.Clock

/// Issues and checks billing tokens with the configured keys.
public service Billing {
    @Inject let config: SigningConfig
    @Inject let clock: Clock

    /// A token for `sub` on `plan`, valid for an hour.
    func issue(sub: String, plan: String) async -> String {
        let claims = BillingClaims(iss: "https://billing.internal", aud: "notes", sub: sub, plan: plan)
        return await Jwt.sign(claims, key: config.signingKey, keyId: "billing-1", expiresIn: 1.hours, now: clock.now)
    }

    /// The claims of a token the billing service signed.
    func check(_ token: String) async throws JwtError -> BillingClaims {
        return try await Jwt.verify(token, key: config.billingKey, issuer: "https://billing.internal", audience: "notes", now: clock.now)
    }
}
