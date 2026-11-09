// One key type per algorithm and role (33-crypto.md, "Signatures"): an Ed25519 public key
// verifies only Ed25519 signatures, an X25519 key can't sign, and an ECDSA key can't agree.
import k.crypto.EcdsaP256PrivateKey
import k.crypto.EcdsaP256Signature
import k.crypto.Ed25519PublicKey
import k.crypto.X25519PrivateKey

func check(key: Ed25519PublicKey, signature: EcdsaP256Signature, ecdsa: EcdsaP256PrivateKey, x: X25519PrivateKey) async -> Bool {
    let ok = await key.verify(signature, of: "m".utf8)
    let signed = await x.sign("m".utf8)
    let shared = await ecdsa.agree(with: x.publicKey)
    return ok
}
