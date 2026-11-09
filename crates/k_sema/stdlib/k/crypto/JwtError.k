import k.http.DecodingError

/// Why verifying a JWS or a JWT failed (`docs/design/33-crypto.md`, "JWS
/// and JWT, for interop").
public enum JwtError implements Error, Equatable {
    /// Not a compact JWS: three base64url parts, a JSON header with an
    /// `alg`, no `crit`; or a JWT whose payload isn't a JSON object.
    case malformed
    /// The header's `alg` isn't the one the key's type verifies (`none`
    /// never is).
    case wrongAlgorithm(String)
    /// No key of the [`JwkSet`] has the header's `kid`, or more than one.
    case unknownKeyId(String?)
    /// The signature doesn't verify with the key.
    case badSignature
    /// `exp` has passed, beyond the leeway.
    case expired
    /// `nbf` hasn't come yet, beyond the leeway.
    case notYetValid
    /// `iss` isn't the expected issuer.
    case wrongIssuer
    /// `aud` doesn't name the expected audience.
    case wrongAudience
    /// The token has no `exp`.
    case missingExpiry
    /// Authentic, but the claims don't decode as the expected type.
    case claims(DecodingError)
}
