// `Secret<K>` has K's operations (33-crypto.md, "Keys in configuration"), so a configured key
// needs no `reveal()`; it forwards the key operations only, never an export, and only for the
// key types.
import k.crypto.AesGcmKey
import k.crypto.Ed25519PrivateKey
import k.crypto.Ed25519Signature
import k.crypto.Sealed
import k.crypto.SharedSecret
import k.crypto.X25519PrivateKey
import k.crypto.X25519PublicKey

func configured(signer: Secret<Ed25519PrivateKey>, vault: Secret<AesGcmKey>, agreement: Secret<X25519PrivateKey>, peer: X25519PublicKey, token: Secret<String>) async {
    let signature: Ed25519Signature = await signer.sign("manifest".utf8)
    let ok = await signer.publicKey.verify(signature, of: "manifest".utf8)
    let sealed: Sealed<String> = await vault.seal("hi")
    let shared: SharedSecret = await agreement.agree(with: peer)
    let raw = vault.exportRaw()
    let notAKey = await token.sign("x".utf8)
}
