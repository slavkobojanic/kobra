/// JSON Web Tokens (RFC 7519) for talking to systems outside K, such as an
/// identity provider's ID token (`docs/design/33-crypto.md`, "JWS and JWT,
/// for interop"). K's own sessions are cookies. The algorithm comes from
/// the key's type, never from the token, and `issuer`, `audience` and
/// `now` are required; `now` comes from an injected `Clock`, so tests
/// control expiry with `TestClock`.
@Native
public class Jwt {
    /// Verifies a JWT's signature, `exp` (required), `nbf`, `iss` and `aud`,
    /// with an Ed25519 key: only `EdDSA` is accepted.
    ///
    /// - Returns: the claims, decoded as `Claims`.
    /// - Throws: [`JwtError`]: the signature's errors, `.missingExpiry`,
    ///   `.expired`, `.notYetValid`, `.wrongIssuer`, `.wrongAudience`, or
    ///   `.claims` when the payload isn't a `Claims`.
    static func verify<Claims>(_ token: String, key: Ed25519PublicKey, issuer: String, audience: String, now: Date, leeway: Duration = 1.minutes) async throws JwtError -> Claims where Claims implements Codable
    /// Verifies a JWT's signature, `exp` (required), `nbf`, `iss` and `aud`,
    /// with a P-256 key: only `ES256`.
    ///
    /// - Returns: the claims, decoded as `Claims`.
    /// - Throws: [`JwtError`]: the signature's errors, `.missingExpiry`,
    ///   `.expired`, `.notYetValid`, `.wrongIssuer`, `.wrongAudience`, or
    ///   `.claims` when the payload isn't a `Claims`.
    static func verify<Claims>(_ token: String, key: EcdsaP256PublicKey, issuer: String, audience: String, now: Date, leeway: Duration = 1.minutes) async throws JwtError -> Claims where Claims implements Codable
    /// Verifies a JWT's signature, `exp` (required), `nbf`, `iss` and `aud`,
    /// with an RSA key: `RS256` or `PS256`.
    ///
    /// - Returns: the claims, decoded as `Claims`.
    /// - Throws: [`JwtError`]: the signature's errors, `.missingExpiry`,
    ///   `.expired`, `.notYetValid`, `.wrongIssuer`, `.wrongAudience`, or
    ///   `.claims` when the payload isn't a `Claims`.
    static func verify<Claims>(_ token: String, key: RsaPublicKey, issuer: String, audience: String, now: Date, leeway: Duration = 1.minutes) async throws JwtError -> Claims where Claims implements Codable
    /// Verifies a JWT's signature, `exp` (required), `nbf`, `iss` and `aud`,
    /// with an HMAC secret, as UTF-8: only `HS256`.
    ///
    /// - Returns: the claims, decoded as `Claims`.
    /// - Throws: [`JwtError`]: the signature's errors, `.missingExpiry`,
    ///   `.expired`, `.notYetValid`, `.wrongIssuer`, `.wrongAudience`, or
    ///   `.claims` when the payload isn't a `Claims`.
    static func verify<Claims>(_ token: String, key: Secret<String>, issuer: String, audience: String, now: Date, leeway: Duration = 1.minutes) async throws JwtError -> Claims where Claims implements Codable
    /// Verifies a JWT's signature, `exp` (required), `nbf`, `iss` and `aud`,
    /// with HMAC key bytes: only `HS256`.
    ///
    /// - Returns: the claims, decoded as `Claims`.
    /// - Throws: [`JwtError`]: the signature's errors, `.missingExpiry`,
    ///   `.expired`, `.notYetValid`, `.wrongIssuer`, `.wrongAudience`, or
    ///   `.claims` when the payload isn't a `Claims`.
    static func verify<Claims>(_ token: String, key: Secret<Bytes>, issuer: String, audience: String, now: Date, leeway: Duration = 1.minutes) async throws JwtError -> Claims where Claims implements Codable
    /// Verifies a JWT's signature, `exp` (required), `nbf`, `iss` and `aud`,
    /// with the key of a set whose `kid` matches the header, at that key's
    /// algorithm.
    ///
    /// - Returns: the claims, decoded as `Claims`.
    /// - Throws: [`JwtError`]: the signature's errors, `.missingExpiry`,
    ///   `.expired`, `.notYetValid`, `.wrongIssuer`, `.wrongAudience`, or
    ///   `.claims` when the payload isn't a `Claims`.
    static func verify<Claims>(_ token: String, key: JwkSet, issuer: String, audience: String, now: Date, leeway: Duration = 1.minutes) async throws JwtError -> Claims where Claims implements Codable

    /// Issues a JWT of `claims` with an Ed25519 key (`EdDSA`).
    ///
    /// - Returns: the compact JWT, with `typ` `JWT`, `kid` when given, and
    ///   `iat` and `exp` set.
    static func sign<Claims>(_ claims: Claims, key: Secret<Ed25519PrivateKey>, keyId: String? = nil, expiresIn: Duration, now: Date) async -> String where Claims implements Codable
    /// Issues a JWT of `claims` with an Ed25519 key held in a variable
    /// (`EdDSA`).
    ///
    /// - Returns: the compact JWT, with `typ` `JWT`, `kid` when given, and
    ///   `iat` and `exp` set.
    static func sign<Claims>(_ claims: Claims, key: Ed25519PrivateKey, keyId: String? = nil, expiresIn: Duration, now: Date) async -> String where Claims implements Codable
    /// Issues a JWT of `claims` with a P-256 key (`ES256`).
    ///
    /// - Returns: the compact JWT, with `typ` `JWT`, `kid` when given, and
    ///   `iat` and `exp` set.
    static func sign<Claims>(_ claims: Claims, key: Secret<EcdsaP256PrivateKey>, keyId: String? = nil, expiresIn: Duration, now: Date) async -> String where Claims implements Codable
    /// Issues a JWT of `claims` with a P-256 key held in a variable (`ES256`).
    ///
    /// - Returns: the compact JWT, with `typ` `JWT`, `kid` when given, and
    ///   `iat` and `exp` set.
    static func sign<Claims>(_ claims: Claims, key: EcdsaP256PrivateKey, keyId: String? = nil, expiresIn: Duration, now: Date) async -> String where Claims implements Codable
    /// Issues a JWT of `claims` with an HMAC secret of at least 32 bytes, as
    /// UTF-8 (`HS256`).
    ///
    /// - Returns: the compact JWT, with `typ` `JWT`, `kid` when given, and
    ///   `iat` and `exp` set.
    static func sign<Claims>(_ claims: Claims, key: Secret<String>, keyId: String? = nil, expiresIn: Duration, now: Date) async -> String where Claims implements Codable
    /// Issues a JWT of `claims` with HMAC key bytes, at least 32 (`HS256`).
    ///
    /// - Returns: the compact JWT, with `typ` `JWT`, `kid` when given, and
    ///   `iat` and `exp` set.
    static func sign<Claims>(_ claims: Claims, key: Secret<Bytes>, keyId: String? = nil, expiresIn: Duration, now: Date) async -> String where Claims implements Codable
}
