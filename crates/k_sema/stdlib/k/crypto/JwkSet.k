/// A JSON Web Key Set, such as an identity provider's `jwks_uri`
/// document (`docs/design/33-crypto.md`, "JWS and JWT, for interop"). It
/// keeps only the public signature keys Kobra supports (Ed25519, ECDSA P-256,
/// RSA of 2,048 bits or more), so it never holds a symmetric key; a token
/// verified with it uses the key whose `kid` matches its header, at that
/// key's own algorithm. Fetching and caching the document is the app's own
/// `HttpClient` call.
@Native
public struct JwkSet {
    /// The `kid` of each key kept, in the document's order.
    var keyIds: List<String>

    /// Reads a JWKS document (`{"keys": [...]}`).
    ///
    /// - Returns: the set, or `nil` if `json` isn't one.
    static func parse(_ json: String) -> JwkSet?
}
