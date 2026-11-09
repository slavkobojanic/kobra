import k.crypto.Ed25519PrivateKey
import k.crypto.Ed25519PublicKey
import k.crypto.Ed25519Signature

/// An export manifest with the signature of the key that made it.
public struct Manifest implements Codable, Equatable {
    let body: String
    let signature: Ed25519Signature
    let signer: Ed25519PublicKey

    /// Signs `body` with a configured key: `Secret<K>` has K's operations.
    static func signed(_ body: String, with key: Secret<Ed25519PrivateKey>) async -> Manifest {
        return Manifest(body: body, signature: await key.sign(body.utf8), signer: key.publicKey)
    }

    /// Whether `trusted` signed this manifest.
    func verify(trusting trusted: Ed25519PublicKey) async -> Bool {
        guard signer == trusted else {
            return false
        }
        return await trusted.verify(signature, of: body.utf8)
    }
}
