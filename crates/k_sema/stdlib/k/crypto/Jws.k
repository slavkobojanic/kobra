/// JSON Web Signatures in the compact serialization (RFC 7515), for JWS
/// that isn't a JWT (`docs/design/33-crypto.md`, "JWS and JWT, for
/// interop"). The algorithm comes from the key's type, never from the
/// token: `alg: none` is never accepted, and a header `alg` that isn't the
/// key's is `.wrongAlgorithm`.
@Native
public class Jws {
    /// Verifies a compact JWS with an Ed25519 key: only `EdDSA` is accepted.
    ///
    /// - Returns: the payload.
    /// - Throws: [`JwtError`]: `.malformed`, `.wrongAlgorithm`, `.unknownKeyId`
    ///   or `.badSignature`.
    static func verify(_ compact: String, key: Ed25519PublicKey) async throws JwtError -> Bytes
    /// Verifies a compact JWS with a P-256 key: only `ES256`.
    ///
    /// - Returns: the payload.
    /// - Throws: [`JwtError`]: `.malformed`, `.wrongAlgorithm`, `.unknownKeyId`
    ///   or `.badSignature`.
    static func verify(_ compact: String, key: EcdsaP256PublicKey) async throws JwtError -> Bytes
    /// Verifies a compact JWS with an RSA key: `RS256` or `PS256`.
    ///
    /// - Returns: the payload.
    /// - Throws: [`JwtError`]: `.malformed`, `.wrongAlgorithm`, `.unknownKeyId`
    ///   or `.badSignature`.
    static func verify(_ compact: String, key: RsaPublicKey) async throws JwtError -> Bytes
    /// Verifies a compact JWS with an HMAC secret, as UTF-8: only `HS256`.
    ///
    /// - Returns: the payload.
    /// - Throws: [`JwtError`]: `.malformed`, `.wrongAlgorithm`, `.unknownKeyId`
    ///   or `.badSignature`.
    static func verify(_ compact: String, key: Secret<String>) async throws JwtError -> Bytes
    /// Verifies a compact JWS with HMAC key bytes: only `HS256`.
    ///
    /// - Returns: the payload.
    /// - Throws: [`JwtError`]: `.malformed`, `.wrongAlgorithm`, `.unknownKeyId`
    ///   or `.badSignature`.
    static func verify(_ compact: String, key: Secret<Bytes>) async throws JwtError -> Bytes
    /// Verifies a compact JWS with the key of a set whose `kid` matches the
    /// header, at that key's algorithm.
    ///
    /// - Returns: the payload.
    /// - Throws: [`JwtError`]: `.malformed`, `.wrongAlgorithm`, `.unknownKeyId`
    ///   or `.badSignature`.
    static func verify(_ compact: String, key: JwkSet) async throws JwtError -> Bytes

    /// Signs `payload` with an Ed25519 key (`EdDSA`).
    ///
    /// - Returns: the compact JWS, whose header names only `alg`.
    static func sign(_ payload: Bytes, key: Secret<Ed25519PrivateKey>) async -> String
    /// Signs `payload` with an Ed25519 key held in a variable (`EdDSA`).
    ///
    /// - Returns: the compact JWS, whose header names only `alg`.
    static func sign(_ payload: Bytes, key: Ed25519PrivateKey) async -> String
    /// Signs `payload` with a P-256 key (`ES256`).
    ///
    /// - Returns: the compact JWS, whose header names only `alg`.
    static func sign(_ payload: Bytes, key: Secret<EcdsaP256PrivateKey>) async -> String
    /// Signs `payload` with a P-256 key held in a variable (`ES256`).
    ///
    /// - Returns: the compact JWS, whose header names only `alg`.
    static func sign(_ payload: Bytes, key: EcdsaP256PrivateKey) async -> String
    /// Signs `payload` with an HMAC secret of at least 32 bytes, as UTF-8
    /// (`HS256`).
    ///
    /// - Returns: the compact JWS, whose header names only `alg`.
    static func sign(_ payload: Bytes, key: Secret<String>) async -> String
    /// Signs `payload` with HMAC key bytes, at least 32 (`HS256`).
    ///
    /// - Returns: the compact JWS, whose header names only `alg`.
    static func sign(_ payload: Bytes, key: Secret<Bytes>) async -> String
}
