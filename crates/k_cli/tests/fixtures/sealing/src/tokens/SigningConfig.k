import k.crypto.Ed25519PrivateKey
import k.crypto.Ed25519PublicKey

/// Keys from the environment: a private key only as a `Secret`.
public config SigningConfig {
    /// Signs export manifests. A PKCS #8 PEM, a JWK or a base64url seed.
    @Env("EXPORT_SIGNING_KEY") let signingKey: Secret<Ed25519PrivateKey>

    /// Verifies tokens from the billing service.
    @Env("BILLING_TOKEN_KEY") let billingKey: Ed25519PublicKey
}
