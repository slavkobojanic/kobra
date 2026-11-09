// A secret or private key isn't `Codable` (33-crypto.md, "The key types"), so it can't be a
// field of a `Codable` struct (and so can't cross an rpc, a `@Load` field, a job or an entity),
// and `Sealed<AesGcmKey>` doesn't compile: keys are wrapped instead. Public keys and the sealed
// forms are `Codable`.
import k.crypto.AesGcmKey
import k.crypto.Ed25519PublicKey
import k.crypto.Envelope
import k.crypto.Sealed
import k.crypto.WrappedKey
import k.crypto.X25519PrivateKey

struct Leaky implements Codable {
    let key: AesGcmKey
}

struct Stored implements Codable {
    let signer: Ed25519PublicKey
    let wrapped: WrappedKey<AesGcmKey>
    let note: Sealed<String>
    let shared: Envelope<String>
}

func sealKey(with key: AesGcmKey, other: X25519PrivateKey) async {
    let sealed: Sealed<X25519PrivateKey> = await key.seal(other)
}
