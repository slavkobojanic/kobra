// Raw key-agreement output isn't a key (33-crypto.md, "Key agreement"): a `SharedSecret` has no
// members and is only input to a key derivation, and HKDF input is never plain `Bytes`.
import k.crypto.AesGcmKey
import k.crypto.SharedSecret
import k.crypto.X25519PrivateKey

func deriveFrom(mine: X25519PrivateKey, theirs: X25519PrivateKey, salt: Bytes) async {
    let shared: SharedSecret = await mine.agree(with: theirs.publicKey)
    let sealed = await shared.seal("text")
    let key: AesGcmKey = shared
    let fromBytes = await AesGcmKey.derive(from: salt, salt: salt, info: "chat/v1")
    let derived = await AesGcmKey.derive(from: shared, salt: salt, info: "chat/v1")
}
